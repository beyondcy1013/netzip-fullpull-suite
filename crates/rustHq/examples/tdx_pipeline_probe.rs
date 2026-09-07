use dllhqarrow_rs::parser::{parse_quote_body, parse_response_header, uncompress_zlib};
use dllhqarrow_rs::{TdxPacket, default_servers, market_for_code};
use std::env;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

const DEFAULT_CODES: &[&str] = &[
    "000001", "000002", "000333", "000651", "000858", "002415", "002594", "300059", "300750",
    "399001", "399006", "600000", "600009", "600030", "600036", "600519", "601318", "601398",
    "601888", "603259",
];

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let servers = arg_value(&args, "--servers")
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| default_servers().into_iter().take(6).collect());
    let chunks = arg_value(&args, "--chunks")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(4)
        .max(1);
    let batch_size = arg_value(&args, "--batch-size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5)
        .max(1);
    let timeout_ms = arg_value(&args, "--timeout-ms")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(6_000);

    let requests = build_quote_requests(chunks, batch_size);
    println!(
        "tdx_pipeline_probe servers={} chunks={} batch_size={} timeout_ms={}",
        servers.len(),
        requests.len(),
        batch_size,
        timeout_ms
    );

    for server in servers {
        match probe_server(&server, &requests, Duration::from_millis(timeout_ms)) {
            Ok(summary) => println!("{summary}"),
            Err(err) => println!("server={server} status=error error={err}"),
        }
    }
}

fn probe_server(
    server: &str,
    requests: &[Vec<(u8, String)>],
    timeout: Duration,
) -> Result<String, String> {
    let mut sequential = connect_and_handshake(server, timeout)?;
    let sequential_started = Instant::now();
    let mut sequential_counts = Vec::with_capacity(requests.len());
    for request in requests {
        let packet = TdxPacket::create_security_quotes_packet(request);
        sequential.write_all(&packet).map_err(io_to_string)?;
        sequential.flush().map_err(io_to_string)?;
        let body = read_response_body(&mut sequential)?;
        let quotes = parse_quote_body(&body)?;
        sequential_counts.push(quotes.len());
        drain_stream(&mut sequential)?;
    }
    let sequential_ms = sequential_started.elapsed().as_millis();
    let _ = sequential.shutdown(Shutdown::Both);

    let mut pipelined = connect_and_handshake(server, timeout)?;
    let write_started = Instant::now();
    for request in requests {
        let packet = TdxPacket::create_security_quotes_packet(request);
        pipelined.write_all(&packet).map_err(io_to_string)?;
    }
    pipelined.flush().map_err(io_to_string)?;
    let write_ms = write_started.elapsed().as_millis();

    let read_started = Instant::now();
    let mut pipeline_counts = Vec::with_capacity(requests.len());
    let mut ordered_matches = 0_usize;
    for (index, expected) in requests.iter().enumerate() {
        let body = read_response_body(&mut pipelined)
            .map_err(|err| format!("pipeline read response {index} failed: {err}"))?;
        let quotes = parse_quote_body(&body)
            .map_err(|err| format!("pipeline parse response {index} failed: {err}"))?;
        let returned_codes = quotes
            .iter()
            .map(|quote| quote.code.as_str())
            .collect::<Vec<_>>();
        let expected_codes = expected
            .iter()
            .map(|(_, code)| code.as_str())
            .collect::<Vec<_>>();
        if returned_codes == expected_codes {
            ordered_matches += 1;
        }
        pipeline_counts.push(quotes.len());
    }
    let pipeline_read_ms = read_started.elapsed().as_millis();
    let pipeline_total_ms = write_started.elapsed().as_millis();
    let _ = pipelined.shutdown(Shutdown::Both);

    let sequential_total_quotes = sequential_counts.iter().sum::<usize>();
    let pipeline_total_quotes = pipeline_counts.iter().sum::<usize>();
    let speedup = if pipeline_total_ms > 0 {
        sequential_ms as f64 / pipeline_total_ms as f64
    } else {
        0.0
    };
    Ok(format!(
        "server={server} status=ok sequential_ms={sequential_ms} pipeline_total_ms={pipeline_total_ms} pipeline_write_ms={write_ms} pipeline_read_ms={pipeline_read_ms} speedup={speedup:.2} sequential_quotes={sequential_total_quotes} pipeline_quotes={pipeline_total_quotes} ordered_matches={ordered_matches}/{} sequential_counts={:?} pipeline_counts={:?}",
        requests.len(),
        sequential_counts,
        pipeline_counts
    ))
}

fn build_quote_requests(chunks: usize, batch_size: usize) -> Vec<Vec<(u8, String)>> {
    let mut out = Vec::with_capacity(chunks);
    for chunk_index in 0..chunks {
        let mut chunk = Vec::with_capacity(batch_size);
        for offset in 0..batch_size {
            let code = DEFAULT_CODES[(chunk_index * batch_size + offset) % DEFAULT_CODES.len()];
            chunk.push((market_for_code(code), code.to_string()));
        }
        out.push(chunk);
    }
    out
}

fn connect_and_handshake(server: &str, timeout: Duration) -> Result<TcpStream, String> {
    let (host, port) = parse_server_endpoint(server);
    let addr = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(io_to_string)?
        .next()
        .ok_or_else(|| format!("unable to resolve {host}:{port}"))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(io_to_string)?;
    stream.set_nodelay(true).map_err(io_to_string)?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(io_to_string)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(io_to_string)?;
    std::thread::sleep(Duration::from_millis(100));

    for packet in [
        TdxPacket::create_handshake_packet_1(),
        TdxPacket::create_handshake_packet_2(),
        TdxPacket::create_handshake_packet_3(),
    ] {
        stream.write_all(&packet).map_err(io_to_string)?;
        stream.flush().map_err(io_to_string)?;
        let response = read_non_empty(&mut stream)?;
        if response.is_empty() {
            return Err("empty handshake response".to_string());
        }
    }
    Ok(stream)
}

fn read_response_body(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let header_bytes = read_exact_bytes(stream, 16)?;
    let header = parse_response_header(&header_bytes)
        .ok_or_else(|| "invalid response header".to_string())?;
    if !header.is_valid() {
        return Err(format!("invalid response sizes header={header:?}"));
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
    match server.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse::<u16>().unwrap_or(7709)),
        None => (server.to_string(), 7709),
    }
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn io_to_string(error: std::io::Error) -> String {
    error.to_string()
}
