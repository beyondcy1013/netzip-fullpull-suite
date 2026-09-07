use crate::batch::BatchHandle;
use crate::models::{KlineType, TaskData};
use crate::service::TdxDataService;
use crate::table::KlineTable;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
pub struct HubStatistics {
    pub total_requests: usize,
    pub completed_requests: usize,
    pub failed_requests: usize,
    pub worker_count: usize,
    pub connected_servers: usize,
    pub cached_symbols: usize,
    pub pending_tasks: usize,
}

pub struct StockDataHub {
    service: Arc<TdxDataService>,
    code_to_worker_index: RwLock<HashMap<String, usize>>,
    data_cache: Mutex<HashMap<String, KlineTable>>,
    active_handles: Mutex<HashMap<String, Arc<BatchHandle>>>,
    worker_count: AtomicUsize,
    initialized: AtomicBool,
    shutting_down: AtomicBool,
    total_requests: AtomicUsize,
    completed_requests: AtomicUsize,
    failed_requests: AtomicUsize,
}

impl StockDataHub {
    pub const DEFAULT_WORKER_COUNT: usize = 100;

    pub fn new(service: Arc<TdxDataService>) -> Arc<Self> {
        Arc::new(Self {
            service,
            code_to_worker_index: RwLock::new(HashMap::new()),
            data_cache: Mutex::new(HashMap::new()),
            active_handles: Mutex::new(HashMap::new()),
            worker_count: AtomicUsize::new(0),
            initialized: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            total_requests: AtomicUsize::new(0),
            completed_requests: AtomicUsize::new(0),
            failed_requests: AtomicUsize::new(0),
        })
    }

    pub fn instance() -> Arc<Self> {
        static INSTANCE: OnceLock<Arc<StockDataHub>> = OnceLock::new();
        INSTANCE
            .get_or_init(|| StockDataHub::new(TdxDataService::instance()))
            .clone()
    }

    pub fn initialize(
        self: &Arc<Self>,
        all_codes: &[String],
        worker_count: usize,
        connection_count: usize,
    ) -> bool {
        let actual_worker_count = worker_count.max(1).min(connection_count.max(1));
        let initialized = self.service.initialize(all_codes, connection_count.max(1));
        if !initialized {
            return false;
        }
        self.worker_count
            .store(actual_worker_count, Ordering::SeqCst);
        self.assign_codes_to_workers(all_codes, actual_worker_count);
        self.initialized.store(true, Ordering::SeqCst);
        self.shutting_down.store(false, Ordering::SeqCst);
        true
    }

    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        {
            let handles = self.active_handles.lock().unwrap();
            for handle in handles.values() {
                handle.cancel();
            }
        }
        self.service.shutdown();
        self.initialized.store(false, Ordering::SeqCst);
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn request_update(self: &Arc<Self>, code: &str, count: usize, kline_type: KlineType) {
        let mut tasks = BTreeMap::new();
        tasks.insert(code.to_string(), count);
        self.request_update_batch(&tasks, kline_type);
    }

    pub fn request_update_batch(
        self: &Arc<Self>,
        tasks: &BTreeMap<String, usize>,
        kline_type: KlineType,
    ) {
        self.total_requests.fetch_add(tasks.len(), Ordering::SeqCst);
        let handle = self.service.start_kline_batch(tasks, kline_type);
        let batch_id = handle.batch_id().to_string();
        self.active_handles
            .lock()
            .unwrap()
            .insert(batch_id.clone(), handle.clone());

        let hub = self.clone();
        thread::spawn(move || {
            while let Some(result) = handle.take_next(Some(Duration::from_millis(100))) {
                match result.data {
                    TaskData::Klines(rows) if result.success => {
                        let table = KlineTable::new(result.stock_code.clone(), rows);
                        hub.data_cache
                            .lock()
                            .unwrap()
                            .insert(result.stock_code.clone(), table);
                        hub.completed_requests.fetch_add(1, Ordering::SeqCst);
                    }
                    _ => {
                        hub.failed_requests.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
            hub.active_handles.lock().unwrap().remove(&batch_id);
        });
    }

    pub fn get_data(&self, code: &str) -> Option<KlineTable> {
        self.data_cache.lock().unwrap().get(code).cloned()
    }

    pub fn get_worker_index(&self, code: &str) -> Option<usize> {
        self.code_to_worker_index.read().unwrap().get(code).copied()
    }

    pub fn get_worker_count(&self) -> usize {
        self.worker_count.load(Ordering::SeqCst)
    }

    pub fn get_total_stock_count(&self) -> usize {
        self.code_to_worker_index.read().unwrap().len()
    }

    pub fn get_connected_server_count(&self) -> usize {
        self.service.get_connected_count()
    }

    pub fn get_pending_task_count(&self) -> usize {
        self.service.get_pending_task_count()
    }

    pub fn get_statistics(&self) -> HubStatistics {
        HubStatistics {
            total_requests: self.total_requests.load(Ordering::SeqCst),
            completed_requests: self.completed_requests.load(Ordering::SeqCst),
            failed_requests: self.failed_requests.load(Ordering::SeqCst),
            worker_count: self.get_worker_count(),
            connected_servers: self.get_connected_server_count(),
            cached_symbols: self.data_cache.lock().unwrap().len(),
            pending_tasks: self.get_pending_task_count(),
        }
    }

    pub fn reset_retry_state(&self) {
        self.failed_requests.store(0, Ordering::SeqCst);
    }

    pub fn wait_until_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.active_handles.lock().unwrap().is_empty() && self.get_pending_task_count() == 0
            {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn assign_codes_to_workers(&self, codes: &[String], worker_count: usize) {
        let mut mapping = self.code_to_worker_index.write().unwrap();
        mapping.clear();
        if worker_count == 0 {
            return;
        }
        for code in codes {
            let worker_index = stable_hash(code) as usize % worker_count;
            mapping.insert(code.clone(), worker_index);
        }
    }
}

fn stable_hash(input: &str) -> u64 {
    let mut state = 0xcbf2_9ce4_8422_2325_u64;
    for byte in input.as_bytes() {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(0x1000_0000_01b3);
    }
    state
}
