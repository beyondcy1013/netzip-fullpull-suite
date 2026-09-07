use crate::market::market_for_code;
use crate::models::{
    FinanceInfo, GenericRow, KlineBar, KlineType, QuoteRecord, Task, TaskData, TaskResult, TaskType,
};
use crate::packet::TdxPacket;
use crate::parser::{
    parse_block_data, parse_block_info_meta_body, parse_company_info_category_body,
    parse_company_info_content_body, parse_finance_info_body, parse_kline_body,
    parse_minute_time_body, parse_quote_body, parse_response_header, parse_security_count_body,
    parse_security_list_body, parse_single_kline_body, uncompress_zlib,
};
use crate::time::{Date, DateTime};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClientMode {
    #[default]
    Mock,
    Real,
}

impl ClientMode {
    pub fn from_env() -> Self {
        match std::env::var("TDX_CLIENT_MODE") {
            Ok(value) if value.eq_ignore_ascii_case("real") => Self::Real,
            _ => Self::Mock,
        }
    }
}

pub trait TdxClient: Send + Sync {
    fn server(&self) -> &str;
    fn connect(&self) -> Result<(), String>;
    fn disconnect(&self);
    fn is_connected(&self) -> bool;
    fn heartbeat(&self) -> Result<(), String>;
    fn execute(&self, task: &Task) -> TaskResult;
    fn execute_with_read_timeout(
        &self,
        task: &Task,
        _read_timeout: Option<Duration>,
    ) -> TaskResult {
        self.execute(task)
    }
    fn execute_quote_pipeline(&self, tasks: &[Task]) -> Vec<TaskResult> {
        tasks.iter().map(|task| self.execute(task)).collect()
    }
}

#[derive(Clone)]
pub struct TdxHqApi {
    client: Arc<dyn TdxClient>,
}

impl TdxHqApi {
    pub fn from_client(client: Arc<dyn TdxClient>) -> Self {
        Self { client }
    }

    pub fn mock(server: impl Into<String>) -> Self {
        Self::from_client(Arc::new(MockClient::new(server)))
    }

    pub fn real(server: impl Into<String>) -> Self {
        Self::from_client(Arc::new(RealClient::new(server)))
    }

    pub fn connect_to_server(&self) -> Result<(), String> {
        self.client.connect()
    }

    pub fn disconnect(&self) {
        self.client.disconnect();
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    pub fn heartbeat(&self) -> Result<(), String> {
        self.client.heartbeat()
    }

    pub fn server(&self) -> &str {
        self.client.server()
    }

    pub fn execute_task(&self, task: &Task) -> TaskResult {
        self.client.execute(task)
    }

    pub fn execute_task_with_read_timeout(
        &self,
        task: &Task,
        read_timeout: Option<Duration>,
    ) -> TaskResult {
        self.client.execute_with_read_timeout(task, read_timeout)
    }

    pub fn execute_quote_pipeline(&self, tasks: &[Task]) -> Vec<TaskResult> {
        self.client.execute_quote_pipeline(tasks)
    }
}

pub struct MockClient {
    server: String,
    connected: AtomicBool,
}

impl MockClient {
    pub fn new(server: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            connected: AtomicBool::new(false),
        }
    }
}

impl TdxClient for MockClient {
    fn server(&self) -> &str {
        &self.server
    }

    fn connect(&self) -> Result<(), String> {
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn disconnect(&self) {
        self.connected.store(false, Ordering::SeqCst);
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn heartbeat(&self) -> Result<(), String> {
        if self.is_connected() {
            Ok(())
        } else {
            Err("mock client is not connected".to_string())
        }
    }

    fn execute(&self, task: &Task) -> TaskResult {
        let started_at = Instant::now();
        let _ = self.connect();
        thread::sleep(Duration::from_millis(8));

        let data = match task.task_type {
            TaskType::Kline => TaskData::Klines(generate_mock_klines(task)),
            TaskType::Quote => TaskData::Quotes(generate_mock_quotes(task)),
            TaskType::SecurityList => TaskData::Generic(generate_security_list(task.market)),
            TaskType::MinuteTime => TaskData::Generic(generate_minute_rows(task)),
            TaskType::BlockInfo => TaskData::Generic(generate_block_rows(task)),
            TaskType::CompanyInfoCategory => TaskData::Generic(generate_company_categories(task)),
            TaskType::CompanyInfoContent => TaskData::Text(generate_company_content(task)),
            TaskType::FinanceInfo => TaskData::Finance(generate_finance_info(task)),
        };

        TaskResult::success(task, data, self.server.clone(), started_at.elapsed())
    }
}

pub struct RealClient {
    server: String,
    connected: AtomicBool,
    state: Mutex<RealState>,
    read_timeout_override: Mutex<Option<Duration>>,
}

struct RealState {
    stream: Option<TcpStream>,
}

impl RealClient {
    const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
    const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
    const READ_TIMEOUT: Duration = Duration::from_secs(8);
    const HANDSHAKE_READ_TIMEOUT: Duration = Duration::from_secs(2);
    const MAX_COUNT_PER_REQUEST: usize = 800;
    const SECURITY_LIST_PAGE_SIZE: usize = 1000;
    const FILE_CHUNK_SIZE: u32 = 30_000;

    pub fn new(server: impl Into<String>) -> Self {
        let server = server.into();
        Self {
            server,
            connected: AtomicBool::new(false),
            state: Mutex::new(RealState { stream: None }),
            read_timeout_override: Mutex::new(None),
        }
    }

    fn ensure_connected(&self) -> Result<(), String> {
        if self.is_connected() {
            return Ok(());
        }
        self.connect()
    }

    fn fetch_kline_task(&self, task: &Task) -> Result<Vec<KlineBar>, String> {
        self.ensure_connected()?;
        let market = market_for_code(&task.stock_code);
        let mut current_start = 0_usize;
        let mut remaining = task.klines_count.max(1);
        let mut rows = Vec::with_capacity(remaining);
        let is_index = is_index_code(&task.stock_code, market);

        while remaining > 0 {
            let batch_count = remaining.min(Self::MAX_COUNT_PER_REQUEST);
            let packet = if is_index {
                TdxPacket::create_index_bars_packet(
                    task.kline_type.as_i32() as u16,
                    u16::from(market),
                    &task.stock_code,
                    current_start as u16,
                    batch_count as u16,
                )
            } else {
                TdxPacket::create_security_bars_packet(
                    task.kline_type.as_i32() as u16,
                    u16::from(market),
                    task.stock_code.as_bytes(),
                    current_start as u16,
                    batch_count as u16,
                )
            };

            let body = self.send_packet_receive_body(&packet)?;
            let mut batch = if is_index {
                parse_single_kline_body(&task.stock_code, task.kline_type.as_i32(), &body, true)?
            } else {
                parse_kline_body(&task.stock_code, task.kline_type.as_i32(), &body)?
            };

            if batch.is_empty() {
                break;
            }

            current_start += batch.len();
            remaining = remaining.saturating_sub(batch.len());
            rows.append(&mut batch);

            if current_start >= task.klines_count || remaining == 0 {
                break;
            }
        }

        rows.sort_by(|left, right| left.datetime.cmp(&right.datetime));
        Ok(rows)
    }

    fn fetch_quote_task(&self, task: &Task) -> Result<Vec<QuoteRecord>, String> {
        self.ensure_connected()?;
        if task.quote_stocks.is_empty() {
            return Ok(Vec::new());
        }
        let packet = TdxPacket::create_security_quotes_packet(&task.quote_stocks);
        let body = self.send_packet_receive_body(&packet)?;
        parse_quote_body(&body)
    }

    fn fetch_quote_pipeline(&self, tasks: &[Task]) -> Vec<TaskResult> {
        if tasks.is_empty() {
            return Vec::new();
        }
        let started_at = Instant::now();
        let mut results = Vec::with_capacity(tasks.len());
        if let Err(error) = self.ensure_connected() {
            return tasks
                .iter()
                .map(|task| {
                    TaskResult::failure(
                        task,
                        error.clone(),
                        self.server.clone(),
                        started_at.elapsed(),
                    )
                })
                .collect();
        }

        let pipeline_result: Result<Vec<Vec<u8>>, String> = {
            let mut state = self.state.lock().unwrap();
            let Some(stream) = state.stream.as_mut() else {
                return pipeline_failure_results(
                    tasks,
                    &self.server,
                    started_at,
                    "tcp stream is not connected".to_string(),
                );
            };
            let read_timeout = self
                .read_timeout_override
                .lock()
                .unwrap()
                .unwrap_or(Self::READ_TIMEOUT);
            if let Err(error) = stream.set_read_timeout(Some(read_timeout)) {
                return pipeline_failure_results(
                    tasks,
                    &self.server,
                    started_at,
                    io_to_string(error),
                );
            }

            write_read_quote_pipeline(stream, tasks)
        };

        let bodies = match pipeline_result {
            Ok(bodies) => bodies,
            Err(error) => {
                self.disconnect();
                return pipeline_failure_results(
                    tasks,
                    &self.server,
                    started_at,
                    format!("pipeline setup failed: {error}"),
                );
            }
        };

        for (index, (task, body)) in tasks.iter().zip(bodies).enumerate() {
            match parse_quote_body(&body) {
                Ok(quotes) => results.push(TaskResult::success(
                    task,
                    TaskData::Quotes(quotes),
                    self.server.clone(),
                    started_at.elapsed(),
                )),
                Err(error) => {
                    self.disconnect();
                    return pipeline_partial_failure_results(
                        tasks,
                        &self.server,
                        started_at,
                        results,
                        index,
                        format!("pipeline parse response {index} failed: {error}"),
                    );
                }
            }
        }
        results
    }

    fn fetch_security_list_task(&self, task: &Task) -> Result<Vec<GenericRow>, String> {
        self.ensure_connected()?;
        let mut start = task.start as usize;
        let mut rows = Vec::new();

        loop {
            let packet = TdxPacket::create_security_list_packet(task.market.into(), start as u16);
            let body = self.send_packet_receive_body(&packet)?;
            let batch = parse_security_list_body(&body, task.market)?;
            let batch_len = batch.len();

            if batch_len == 0 {
                break;
            }

            rows.extend(batch);

            if batch_len < Self::SECURITY_LIST_PAGE_SIZE {
                break;
            }

            start += batch_len;
        }

        Ok(rows)
    }

    fn fetch_company_info_category_task(&self, task: &Task) -> Result<Vec<GenericRow>, String> {
        self.ensure_connected()?;
        let packet =
            TdxPacket::create_company_info_category_packet(task.market.into(), &task.stock_code);
        let body = self.send_packet_receive_body(&packet)?;
        parse_company_info_category_body(&body)
    }

    fn fetch_minute_time_task(&self, task: &Task) -> Result<Vec<GenericRow>, String> {
        self.ensure_connected()?;
        let packet =
            TdxPacket::create_minute_time_data_packet(task.market.into(), &task.stock_code);
        let body = self.send_packet_receive_body(&packet)?;
        parse_minute_time_body(&body)
    }

    fn fetch_company_info_content_task(&self, task: &Task) -> Result<String, String> {
        self.ensure_connected()?;
        let mut full_content = String::new();
        let mut current_start = task.start;
        let mut remaining = task.length.max(1);

        while remaining > 0 {
            let request_size = remaining.min(Self::FILE_CHUNK_SIZE);
            let packet = TdxPacket::create_company_info_content_packet(
                task.market.into(),
                &task.stock_code,
                &task.filename,
                current_start,
                request_size,
            );
            let body = self.send_packet_receive_body(&packet)?;
            let chunk = parse_company_info_content_body(&body)?;
            if !chunk.is_empty() {
                full_content.push_str(&chunk);
            }

            current_start = current_start.saturating_add(request_size);
            remaining = remaining.saturating_sub(request_size);
        }

        Ok(full_content)
    }

    fn fetch_finance_info_task(&self, task: &Task) -> Result<FinanceInfo, String> {
        self.ensure_connected()?;
        let packet = TdxPacket::create_finance_info_packet(task.market, &task.stock_code);
        let body = self.send_packet_receive_body(&packet)?;
        parse_finance_info_body(&body)
    }

    fn fetch_block_info_task(&self, task: &Task) -> Result<Vec<GenericRow>, String> {
        self.ensure_connected()?;
        let meta_packet = TdxPacket::create_block_info_meta_packet(&task.block_file);
        let meta_body = self.send_packet_receive_body(&meta_packet)?;
        let (file_size, _) = parse_block_info_meta_body(&meta_body)?;
        if file_size == 0 {
            return Ok(Vec::new());
        }

        let mut start = 0_u32;
        let mut file_content = Vec::with_capacity(file_size as usize);
        while start < file_size {
            let packet =
                TdxPacket::create_block_info_packet(&task.block_file, start, Self::FILE_CHUNK_SIZE);
            let mut body = self.send_packet_receive_body(&packet)?;
            if body.len() > 4 {
                body.drain(..4);
            } else {
                body.clear();
            }
            if body.is_empty() {
                break;
            }

            file_content.extend_from_slice(&body);
            if file_content.len() >= file_size as usize {
                break;
            }

            start = start.saturating_add(Self::FILE_CHUNK_SIZE);
        }

        file_content.truncate(file_size as usize);
        parse_block_data(&file_content)
    }

    fn send_packet_receive_body(&self, packet: &[u8]) -> Result<Vec<u8>, String> {
        let mut state = self.state.lock().unwrap();
        let stream = state
            .stream
            .as_mut()
            .ok_or_else(|| "tcp stream is not connected".to_string())?;
        let read_timeout = self
            .read_timeout_override
            .lock()
            .unwrap()
            .unwrap_or(Self::READ_TIMEOUT);
        stream
            .set_read_timeout(Some(read_timeout))
            .map_err(io_to_string)?;

        drain_stream(stream)?;
        stream.write_all(packet).map_err(io_to_string)?;
        stream.flush().map_err(io_to_string)?;

        let header_bytes = read_exact_bytes(stream, 16)?;
        let header = parse_response_header(&header_bytes)
            .ok_or_else(|| "invalid response header".to_string())?;
        if !header.is_valid() {
            return Err("invalid response sizes".to_string());
        }

        let zipped_body = read_exact_bytes(stream, header.zip_size as usize)?;
        if header.zip_size == header.unzip_size {
            Ok(zipped_body)
        } else {
            uncompress_zlib(&zipped_body, header.unzip_size as usize)
        }
    }

    fn execute_with_retry<T, F, G>(
        &self,
        task: &Task,
        started_at: Instant,
        fetch: F,
        map: G,
    ) -> TaskResult
    where
        F: Fn(&Self, &Task) -> Result<T, String>,
        G: Fn(T) -> TaskData,
    {
        let mut last_error = "unknown real client error".to_string();

        for attempt in 0..2 {
            if attempt > 0 {
                self.disconnect();
            }

            match fetch(self, task) {
                Ok(value) => {
                    return TaskResult::success(
                        task,
                        map(value),
                        self.server.clone(),
                        started_at.elapsed(),
                    );
                }
                Err(error) => {
                    last_error = error;
                }
            }
        }

        self.disconnect();
        TaskResult::failure(task, last_error, self.server.clone(), started_at.elapsed())
    }

    fn perform_handshake(stream: &mut TcpStream) -> Result<(), String> {
        stream
            .set_write_timeout(Some(Self::WRITE_TIMEOUT))
            .map_err(io_to_string)?;
        stream
            .set_read_timeout(Some(Self::HANDSHAKE_READ_TIMEOUT))
            .map_err(io_to_string)?;

        for packet in [
            TdxPacket::create_handshake_packet_1(),
            TdxPacket::create_handshake_packet_2(),
            TdxPacket::create_handshake_packet_3(),
        ] {
            stream.write_all(&packet).map_err(io_to_string)?;
            stream.flush().map_err(io_to_string)?;
            let response = read_non_empty(stream)?;
            if response.is_empty() {
                return Err("empty handshake response".to_string());
            }
        }

        stream
            .set_read_timeout(Some(Self::READ_TIMEOUT))
            .map_err(io_to_string)?;
        Ok(())
    }
}

impl TdxClient for RealClient {
    fn server(&self) -> &str {
        &self.server
    }

    fn connect(&self) -> Result<(), String> {
        if self.is_connected() {
            return Ok(());
        }

        let (host, port) = parse_server_endpoint(&self.server);
        let addr = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(io_to_string)?
            .next()
            .ok_or_else(|| format!("unable to resolve server {host}:{port}"))?;

        let mut stream =
            TcpStream::connect_timeout(&addr, Self::CONNECT_TIMEOUT).map_err(io_to_string)?;
        stream.set_nodelay(true).map_err(io_to_string)?;
        stream
            .set_write_timeout(Some(Self::WRITE_TIMEOUT))
            .map_err(io_to_string)?;
        stream
            .set_read_timeout(Some(Self::READ_TIMEOUT))
            .map_err(io_to_string)?;

        thread::sleep(Duration::from_millis(100));
        Self::perform_handshake(&mut stream)?;

        let mut state = self.state.lock().unwrap();
        state.stream = Some(stream);
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn disconnect(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(stream) = state.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        self.connected.store(false, Ordering::SeqCst);
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn heartbeat(&self) -> Result<(), String> {
        self.ensure_connected()?;
        let body = self.send_packet_receive_body(&TdxPacket::create_security_count_packet(0))?;
        let count = parse_security_count_body(&body)
            .ok_or_else(|| "invalid security count response".to_string())?;
        if count > 0 {
            Ok(())
        } else {
            Err("security count response was zero".to_string())
        }
    }

    fn execute(&self, task: &Task) -> TaskResult {
        let started_at = Instant::now();

        match task.task_type {
            TaskType::Kline => {
                self.execute_with_retry(task, started_at, Self::fetch_kline_task, TaskData::Klines)
            }
            TaskType::Quote => {
                self.execute_with_retry(task, started_at, Self::fetch_quote_task, TaskData::Quotes)
            }
            TaskType::SecurityList => self.execute_with_retry(
                task,
                started_at,
                Self::fetch_security_list_task,
                TaskData::Generic,
            ),
            TaskType::CompanyInfoCategory => self.execute_with_retry(
                task,
                started_at,
                Self::fetch_company_info_category_task,
                TaskData::Generic,
            ),
            TaskType::CompanyInfoContent => self.execute_with_retry(
                task,
                started_at,
                Self::fetch_company_info_content_task,
                TaskData::Text,
            ),
            TaskType::FinanceInfo => self.execute_with_retry(
                task,
                started_at,
                Self::fetch_finance_info_task,
                TaskData::Finance,
            ),
            TaskType::BlockInfo => self.execute_with_retry(
                task,
                started_at,
                Self::fetch_block_info_task,
                TaskData::Generic,
            ),
            TaskType::MinuteTime => self.execute_with_retry(
                task,
                started_at,
                Self::fetch_minute_time_task,
                TaskData::Generic,
            ),
        }
    }

    fn execute_with_read_timeout(&self, task: &Task, read_timeout: Option<Duration>) -> TaskResult {
        {
            let mut current = self.read_timeout_override.lock().unwrap();
            *current = read_timeout;
        }
        let result = self.execute(task);
        {
            let mut current = self.read_timeout_override.lock().unwrap();
            *current = None;
        }
        result
    }

    fn execute_quote_pipeline(&self, tasks: &[Task]) -> Vec<TaskResult> {
        self.fetch_quote_pipeline(tasks)
    }
}

fn pipeline_failure_results(
    tasks: &[Task],
    server: &str,
    started_at: Instant,
    error: String,
) -> Vec<TaskResult> {
    tasks
        .iter()
        .map(|task| {
            TaskResult::failure(
                task,
                error.clone(),
                server.to_string(),
                started_at.elapsed(),
            )
        })
        .collect()
}

fn pipeline_partial_failure_results(
    tasks: &[Task],
    server: &str,
    started_at: Instant,
    mut results: Vec<TaskResult>,
    failed_index: usize,
    error: String,
) -> Vec<TaskResult> {
    for task in tasks.iter().skip(failed_index) {
        results.push(TaskResult::failure(
            task,
            error.clone(),
            server.to_string(),
            started_at.elapsed(),
        ));
    }
    results
}

fn write_read_quote_pipeline(
    stream: &mut TcpStream,
    tasks: &[Task],
) -> Result<Vec<Vec<u8>>, String> {
    drain_stream(stream)?;
    for task in tasks {
        let packet = TdxPacket::create_security_quotes_packet(&task.quote_stocks);
        stream
            .write_all(&packet)
            .map_err(|error| format!("pipeline write failed: {}", io_to_string(error)))?;
    }
    stream
        .flush()
        .map_err(|error| format!("pipeline flush failed: {}", io_to_string(error)))?;

    let mut bodies = Vec::with_capacity(tasks.len());
    for index in 0..tasks.len() {
        match read_response_body(stream) {
            Ok(body) => bodies.push(body),
            Err(error) => return Err(format!("pipeline read response {index} failed: {error}")),
        }
    }
    Ok(bodies)
}

fn read_response_body(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let header_bytes = read_exact_bytes(stream, 16)?;
    let header = parse_response_header(&header_bytes)
        .ok_or_else(|| "invalid response header".to_string())?;
    if !header.is_valid() {
        return Err("invalid response sizes".to_string());
    }
    let zipped_body = read_exact_bytes(stream, header.zip_size as usize)?;
    if header.zip_size == header.unzip_size {
        Ok(zipped_body)
    } else {
        uncompress_zlib(&zipped_body, header.unzip_size as usize)
    }
}

fn read_non_empty(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut buffer = vec![0_u8; 2048];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return Err("connection closed while reading".to_string()),
            Ok(size) => return Ok(buffer[..size].to_vec()),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_to_string(error)),
        }
    }
}

fn read_exact_bytes(stream: &mut TcpStream, len: usize) -> Result<Vec<u8>, String> {
    let mut buffer = vec![0_u8; len];
    stream.read_exact(&mut buffer).map_err(io_to_string)?;
    Ok(buffer)
}

fn drain_stream(stream: &mut TcpStream) -> Result<(), String> {
    stream.set_nonblocking(true).map_err(io_to_string)?;
    let mut buffer = [0_u8; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => {
                let _ = stream.set_nonblocking(false);
                return Err(io_to_string(error));
            }
        }
    }
    stream.set_nonblocking(false).map_err(io_to_string)?;
    Ok(())
}

fn parse_server_endpoint(server: &str) -> (String, u16) {
    match server.split_once(':') {
        Some((host, port)) => (host.to_string(), port.parse::<u16>().unwrap_or(7709)),
        None => (server.to_string(), 7709),
    }
}

fn io_to_string(error: impl ToString) -> String {
    error.to_string()
}

fn is_index_code(code: &str, market: u8) -> bool {
    code.starts_with("399") || code.starts_with("880") || (market == 1 && code.starts_with("000"))
}

fn generate_mock_klines(task: &Task) -> Vec<KlineBar> {
    let count = task.klines_count.max(1);
    let seed = stable_hash(&(task.stock_code.clone() + &task.key()));
    let mut previous_close = 8.0 + (seed % 4_000) as f64 / 100.0;
    let mut rows = Vec::with_capacity(count);

    for index in 0..count {
        let dt = if task.kline_type.is_minute() {
            format_minute_datetime(index, count)
        } else {
            format_daily_datetime(index)
        };
        let drift_basis = ((seed >> (index % 8)) + index as u64 * 13) % 19;
        let drift = drift_basis as f64 / 1_000.0 - 0.008;
        let open = previous_close;
        let close = (open * (1.0 + drift)).max(0.1);
        let high = open.max(close) + (drift_basis % 5) as f64 * 0.03;
        let low = open.min(close) - ((drift_basis + 2) % 4) as f64 * 0.02;
        let volume = 10_000.0 + ((seed + index as u64 * 97) % 80_000) as f64;
        let amount = close * volume;
        let mut bar = KlineBar::new(
            task.stock_code.clone(),
            dt.clone(),
            open,
            high,
            low.max(0.01),
            close,
            volume,
            amount,
        );
        bar.timestamp = DateTime::parse(&dt).map(|value| {
            crate::time::civil_to_days(value.date.year, value.date.month, value.date.day) * 86_400
                + i64::from(value.hour) * 3600
                + i64::from(value.minute) * 60
                + i64::from(value.second)
        });
        rows.push(bar);
        previous_close = close;
    }

    rows
}

fn generate_mock_quotes(task: &Task) -> Vec<QuoteRecord> {
    task.quote_stocks
        .iter()
        .enumerate()
        .map(|(index, (market, code))| {
            let seed = stable_hash(&(code.clone() + &index.to_string()));
            let last_close = 8.0 + (seed % 5_000) as f64 / 100.0;
            let price = last_close * (1.0 + (seed % 23) as f64 / 10_000.0);
            QuoteRecord {
                market: *market,
                code: code.clone(),
                active1: 0,
                price,
                open: last_close * 0.998,
                high: price * 1.003,
                low: price * 0.996,
                last_close,
                volume: 100_000.0 + (seed % 200_000) as f64,
                current_volume: 1_000.0 + (seed % 3_000) as f64,
                amount: price * (100_000.0 + (seed % 200_000) as f64),
                timestamp: 1_741_564_800 + index as i64,
                servertime: None,
            }
        })
        .collect()
}

fn generate_security_list(market: u8) -> Vec<GenericRow> {
    let prefixes = match market {
        1 => ["600000", "600519", "688001"],
        2 => ["830001", "831526", "920002"],
        _ => ["000001", "002415", "300059"],
    };
    prefixes
        .into_iter()
        .enumerate()
        .map(|(index, code)| {
            let mut row = GenericRow::new();
            row.insert("market".to_string(), market.to_string());
            row.insert("code".to_string(), code.to_string());
            row.insert("name".to_string(), format!("MockSecurity{}", index + 1));
            row
        })
        .collect()
}

fn generate_minute_rows(task: &Task) -> Vec<GenericRow> {
    generate_mock_klines(&Task::kline(&task.stock_code, 20, KlineType::Kline1Min))
        .into_iter()
        .map(|row| {
            let mut item = GenericRow::new();
            item.insert(
                "time".to_string(),
                row.datetime.split(' ').nth(1).unwrap_or("").to_string(),
            );
            item.insert("price".to_string(), format!("{:.3}", row.close));
            item.insert("vol".to_string(), format!("{:.0}", row.volume));
            item
        })
        .collect()
}

fn generate_block_rows(task: &Task) -> Vec<GenericRow> {
    let mut row = GenericRow::new();
    row.insert("blockname".to_string(), task.block_file.clone());
    row.insert("block_type".to_string(), "1".to_string());
    row.insert("stock_count".to_string(), "3".to_string());
    row.insert("code_list".to_string(), "000001,000002,600000".to_string());
    vec![row]
}

fn generate_company_categories(task: &Task) -> Vec<GenericRow> {
    let sections = [
        ("Profile", "profile.txt", 0_u32, 256_u32),
        ("Shareholders", "shareholders.txt", 256_u32, 512_u32),
        ("Finance", "finance.txt", 768_u32, 512_u32),
    ];
    sections
        .into_iter()
        .map(|(name, filename, start, length)| {
            let mut row = GenericRow::new();
            row.insert("name".to_string(), name.to_string());
            row.insert("filename".to_string(), filename.to_string());
            row.insert("start".to_string(), start.to_string());
            row.insert("length".to_string(), length.to_string());
            row.insert("code".to_string(), task.stock_code.clone());
            row
        })
        .collect()
}

fn generate_company_content(task: &Task) -> String {
    format!(
        "Mock F10 content for {} on {} from {} bytes starting at {}.",
        task.stock_code, task.filename, task.length, task.start
    )
}

fn generate_finance_info(task: &Task) -> FinanceInfo {
    let mut fields = GenericRow::new();
    fields.insert("liutongguben".to_string(), "105000.0".to_string());
    fields.insert("zongguben".to_string(), "210000.0".to_string());
    fields.insert("jingzichan".to_string(), "4200000.0".to_string());
    fields.insert("zhuyingshouru".to_string(), "980000.0".to_string());
    fields.insert("ipo_date".to_string(), "20100416".to_string());
    FinanceInfo {
        market: task.market,
        code: task.stock_code.clone(),
        fields,
    }
}

fn format_daily_datetime(index: usize) -> String {
    Date::new(2026, 1, 1).add_days(index as i32).format_iso() + " 15:00:00"
}

fn format_minute_datetime(index: usize, count: usize) -> String {
    let base_date = Date::new(2026, 3, 2);
    let start_index = count.saturating_sub(index + 1);
    let trading_minutes_per_day = 240;
    let day_offset = start_index / trading_minutes_per_day;
    let minute_offset = start_index % trading_minutes_per_day;
    let total_minutes = if minute_offset < 120 {
        9 * 60 + 30 + minute_offset
    } else {
        13 * 60 + (minute_offset - 120)
    };
    let hour = (total_minutes / 60) as u8;
    let minute = (total_minutes % 60) as u8;
    DateTime::new(base_date.add_days(day_offset as i32), hour, minute, 0).format(true)
}

fn stable_hash(input: &str) -> u64 {
    let mut state = 0xcbf2_9ce4_8422_2325_u64;
    for byte in input.as_bytes() {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(0x1000_0000_01b3);
    }
    state
}

pub fn quote_task_from_codes(codes: &[String]) -> Task {
    let stocks = codes
        .iter()
        .map(|code| (market_for_code(code), code.clone()))
        .collect::<Vec<_>>();
    Task::quote(stocks)
}
