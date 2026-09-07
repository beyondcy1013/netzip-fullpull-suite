use crate::api::{ClientMode, quote_task_from_codes};
use crate::batch::BatchHandle;
use crate::manager::{ManagerConfig, ManagerStatistics, ServerHealthSnapshot, TdxDataManager};
use crate::market::{market_for_code, pick_servers};
use crate::models::{FinanceInfo, GenericRow, KlineType, QuoteRecord, Task, TaskData, TaskResult};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

#[derive(Clone)]
pub struct TdxDataService {
    manager: Arc<TdxDataManager>,
    initialized: Arc<AtomicBool>,
    shutting_down: Arc<AtomicBool>,
    exclusive_owner: Arc<Mutex<Option<String>>>,
}

impl Default for TdxDataService {
    fn default() -> Self {
        Self {
            manager: Arc::new(TdxDataManager::new()),
            initialized: Arc::new(AtomicBool::new(false)),
            shutting_down: Arc::new(AtomicBool::new(false)),
            exclusive_owner: Arc::new(Mutex::new(None)),
        }
    }
}

impl TdxDataService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn instance() -> Arc<Self> {
        static INSTANCE: OnceLock<Arc<TdxDataService>> = OnceLock::new();
        INSTANCE
            .get_or_init(|| Arc::new(TdxDataService::default()))
            .clone()
    }

    pub fn initialize(&self, stock_codes: &[String], min_workers: usize) -> bool {
        let worker_count = min_workers.max(1);
        let candidate_count =
            worker_count.saturating_mul(self.manager.config().candidate_multiplier.max(1));
        let servers = pick_servers(candidate_count);
        let initialized = self.manager.initialize(&servers, stock_codes, worker_count);
        if initialized {
            self.initialized.store(true, Ordering::SeqCst);
            self.shutting_down.store(false, Ordering::SeqCst);
        }
        initialized
    }

    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        self.manager.shutdown();
        self.initialized.store(false, Ordering::SeqCst);
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn set_client_mode(&self, client_mode: ClientMode) {
        self.manager.set_client_mode(client_mode);
    }

    pub fn client_mode(&self) -> ClientMode {
        self.manager.client_mode()
    }

    pub fn set_manager_config(&self, config: ManagerConfig) {
        self.manager.set_config(config);
    }

    pub fn manager_config(&self) -> ManagerConfig {
        self.manager.config()
    }

    pub fn request_min1_data_batch(&self, stock_tasks: &BTreeMap<String, usize>) -> bool {
        self.manager
            .schedule_tasks(stock_tasks, KlineType::Kline1Min)
    }

    pub fn request_daily_data_batch(&self, stock_tasks: &BTreeMap<String, usize>) -> bool {
        self.manager
            .schedule_tasks(stock_tasks, KlineType::KlineDaily)
    }

    pub fn request_batch(
        &self,
        stock_tasks: &BTreeMap<String, usize>,
        kline_type: KlineType,
        owner: &str,
    ) -> bool {
        self.manager
            .schedule_batch(stock_tasks, kline_type, owner.to_string())
    }

    pub fn request_quote_batch(
        &self,
        stocks: &[(u8, String)],
        batch_size: usize,
        owner: &str,
    ) -> bool {
        self.manager
            .schedule_quote_batch(stocks, batch_size, owner.to_string())
    }

    pub fn request_generic_batch(&self, tasks: &[Task], owner: &str) -> bool {
        self.manager
            .schedule_generic_batch(tasks, owner.to_string())
    }

    pub fn wait_for_all_tasks(&self, timeout: Duration) -> bool {
        self.manager.wait_for_all_tasks(timeout)
    }

    pub fn wait_for_batch(&self, owner: &str, timeout: Duration) -> bool {
        self.manager.wait_for_batch(owner, timeout)
    }

    pub fn get_completed_results(&self) -> Vec<TaskResult> {
        self.manager.get_completed_results()
    }

    pub fn get_batch_results(&self, owner: &str) -> Vec<TaskResult> {
        self.manager.get_batch_results(owner)
    }

    pub fn take_batch_results(&self, owner: &str) -> Vec<TaskResult> {
        self.manager.take_batch_results(owner)
    }

    pub fn get_batch_progress(&self, owner: &str) -> (usize, usize) {
        self.manager.get_batch_progress(owner)
    }

    pub fn start_kline_batch(
        &self,
        stock_tasks: &BTreeMap<String, usize>,
        kline_type: KlineType,
    ) -> Arc<BatchHandle> {
        self.manager.start_kline_batch(stock_tasks, kline_type)
    }

    pub fn start_quote_batch(
        &self,
        stocks: &[(u8, String)],
        batch_size: usize,
    ) -> Arc<BatchHandle> {
        self.manager.start_quote_batch(stocks, batch_size)
    }

    pub fn start_generic_batch(&self, tasks: &[Task]) -> Arc<BatchHandle> {
        self.manager.start_generic_batch(tasks)
    }

    pub fn cancel_batch(&self, batch_id: &str) {
        self.manager.cancel_batch(batch_id);
    }

    pub fn drop_batch(&self, batch_id: &str) {
        self.manager.drop_batch(batch_id);
    }

    pub fn get_statistics(&self) -> ManagerStatistics {
        self.manager.get_statistics()
    }

    pub fn get_server_health(&self) -> Vec<ServerHealthSnapshot> {
        self.manager.get_server_health()
    }

    pub fn try_acquire_lock(&self, owner: &str) -> bool {
        let mut lock = self.exclusive_owner.lock().unwrap();
        match lock.as_ref() {
            Some(current) if current != owner => false,
            _ => {
                *lock = Some(owner.to_string());
                true
            }
        }
    }

    pub fn release_lock(&self, owner: &str) {
        let mut lock = self.exclusive_owner.lock().unwrap();
        if lock.as_deref() == Some(owner) {
            *lock = None;
        }
    }

    pub fn get_finance_info(&self, market: u8, code: &str) -> FinanceInfo {
        let task = Task::finance_info(market, code.to_string());
        match self
            .run_single_generic_with_timeout(&task, Duration::from_secs(8))
            .data
        {
            TaskData::Finance(value) => value,
            _ => FinanceInfo::default(),
        }
    }

    pub fn get_company_info_category(&self, market: u8, code: &str) -> Vec<GenericRow> {
        let task = Task::company_info_category(market, code.to_string());
        match self
            .run_single_generic_with_timeout(&task, Duration::from_secs(8))
            .data
        {
            TaskData::Generic(values) => values,
            _ => Vec::new(),
        }
    }

    pub fn get_company_info_content(
        &self,
        market: u8,
        code: &str,
        filename: &str,
        start: u32,
        length: u32,
    ) -> String {
        let task = Task::company_info_content(
            market,
            code.to_string(),
            filename.to_string(),
            start,
            length,
        );
        match self
            .run_single_generic_with_timeout(&task, Duration::from_secs(15))
            .data
        {
            TaskData::Text(value) => value,
            _ => String::new(),
        }
    }

    pub fn get_block_info(&self, block_file: &str) -> Vec<GenericRow> {
        let task = Task::block_info(block_file.to_string());
        match self
            .run_single_generic_with_timeout(&task, Duration::from_secs(15))
            .data
        {
            TaskData::Generic(values) => values,
            _ => Vec::new(),
        }
    }

    pub fn get_minute_time_data(&self, market: u8, code: &str) -> Vec<GenericRow> {
        let task = Task::minute_time(market, code.to_string());
        match self
            .run_single_generic_with_timeout(&task, Duration::from_secs(8))
            .data
        {
            TaskData::Generic(values) => values,
            _ => Vec::new(),
        }
    }

    pub fn get_merged_company_info(&self, code: &str, categories: &[String]) -> String {
        let market = market_for_code(code);
        let all_categories = self.get_company_info_category(market, code);
        if all_categories.is_empty() {
            return "无法获取公司信息目录".to_string();
        }

        let mut merged = Vec::new();
        for category in all_categories {
            let name = category.get("name").cloned().unwrap_or_default();
            if !categories.is_empty() && !categories.iter().any(|value| name.contains(value)) {
                continue;
            }

            let filename = category.get("filename").cloned().unwrap_or_default();
            let start = category
                .get("start")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            let length = category
                .get("length")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);

            let content = self.get_company_info_content(market, code, &filename, start, length);
            if content.is_empty() {
                continue;
            }

            merged.push(format!(
                "【{name}】\n================================================================================\n{content}\n================================================================================"
            ));
        }

        if merged.is_empty() {
            "未找到指定类别的公司信息".to_string()
        } else {
            merged.join("\n\n")
        }
    }

    pub fn get_quotes_v(&self, stock_codes: &[String]) -> Vec<QuoteRecord> {
        self.get_quotes(stock_codes)
    }

    pub fn get_quotes(&self, stock_codes: &[String]) -> Vec<QuoteRecord> {
        let task = quote_task_from_codes(stock_codes);
        match self.run_single_generic(&task).data {
            TaskData::Quotes(values) => values,
            _ => Vec::new(),
        }
    }

    pub fn get_security_list(&self, market: u8) -> Vec<GenericRow> {
        let task = Task::security_list(market);
        match self
            .run_single_generic_with_timeout(&task, Duration::from_secs(15))
            .data
        {
            TaskData::Generic(values) => values,
            _ => Vec::new(),
        }
    }

    pub fn get_all_security_list(&self) -> Vec<GenericRow> {
        let mut rows = self.get_security_list(0);
        rows.extend(self.get_security_list(1));
        rows
    }

    pub fn get_connected_count(&self) -> usize {
        self.manager.get_connected_thread_count()
    }

    pub fn get_pending_task_count(&self) -> usize {
        self.manager.get_pending_task_count()
    }

    pub fn is_busy(&self) -> bool {
        self.get_pending_task_count() > 0
    }

    pub fn manager(&self) -> Arc<TdxDataManager> {
        self.manager.clone()
    }

    fn run_single_generic(&self, task: &Task) -> TaskResult {
        self.run_single_generic_with_timeout(task, Duration::from_secs(3))
    }

    fn run_single_generic_with_timeout(&self, task: &Task, timeout: Duration) -> TaskResult {
        let handle = self.manager.start_generic_batch(std::slice::from_ref(task));
        let _ = handle.wait_produced(Some(timeout));
        handle.try_take().unwrap_or_else(|| {
            TaskResult::failure(task, "missing result", "service", Duration::ZERO)
        })
    }
}

impl Drop for TdxDataService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
