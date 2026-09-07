use crate::api::{ClientMode, TdxHqApi};
use crate::batch::BatchHandle;
use crate::market::{market_for_code, pick_servers};
use crate::models::{KlineType, Task, TaskResult, TaskType};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default)]
pub struct ManagerStatistics {
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub connected_threads: usize,
    pub pending_tasks: usize,
    pub batch_count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct ServerHealthSnapshot {
    pub server: String,
    pub selected: bool,
    pub validated: bool,
    pub connected: bool,
    pub success_count: usize,
    pub failure_count: usize,
    pub consecutive_failures: usize,
    pub consecutive_slow: usize,
    pub validation_latency_ms: Option<u64>,
    pub last_latency_ms: Option<u64>,
    pub avg_latency_ms: Option<f64>,
    pub avg_quote_ms_per_code: Option<f64>,
    pub blacklisted: bool,
    pub cooldown_remaining_ms: u64,
    pub last_error: String,
}

#[derive(Clone, Debug, Default)]
pub struct WorkerActivitySnapshot {
    pub server: String,
    pub connected: bool,
    pub active: bool,
    pub active_batch_id: Option<String>,
    pub active_task_id: Option<String>,
    pub active_task_type: Option<TaskType>,
    pub active_quote_count: usize,
    pub active_elapsed_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ManagerConfig {
    pub heartbeat_interval: Duration,
    pub connect_retry_delay: Duration,
    pub candidate_multiplier: usize,
    pub failure_cooldown_threshold: usize,
    pub failure_cooldown: Duration,
    pub failure_cooldown_max: Duration,
    pub slow_latency_threshold_ms: u64,
    pub slow_cooldown_threshold: usize,
    pub slow_cooldown: Duration,
    pub failure_penalty_ms: f64,
    pub consecutive_failure_penalty_ms: f64,
    pub slow_penalty_ms: f64,
    pub startup_blacklist_latency_threshold_ms: u64,
    pub startup_benchmark_quote_count: usize,
    pub startup_speed_filter_ratio: f64,
    pub startup_speed_filter_min_workers: usize,
    pub quote_default_timeout_per_code_ms: u64,
    pub quote_min_timeout_ms: u64,
    pub quote_slow_max_requeues: usize,
    pub quote_hedge_after_ms: u64,
    pub quote_hedge_max_fraction: f64,
    pub quote_pipeline_window: usize,
    pub quote_tail_slow_worker_remaining_fraction: f64,
    pub quote_tail_slow_worker_fraction: f64,
    pub quote_tail_slow_worker_defer_wait_ms: u64,
    pub health_history_path: Option<PathBuf>,
    pub health_persist_interval: Duration,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(10),
            connect_retry_delay: Duration::from_millis(250),
            candidate_multiplier: 3,
            failure_cooldown_threshold: 2,
            failure_cooldown: Duration::from_secs(15),
            failure_cooldown_max: Duration::from_secs(5 * 60),
            slow_latency_threshold_ms: 2_500,
            slow_cooldown_threshold: 3,
            slow_cooldown: Duration::from_secs(10),
            failure_penalty_ms: 500.0,
            consecutive_failure_penalty_ms: 1_000.0,
            slow_penalty_ms: 250.0,
            startup_blacklist_latency_threshold_ms: 500,
            startup_benchmark_quote_count: 10,
            startup_speed_filter_ratio: 0.0,
            startup_speed_filter_min_workers: 0,
            quote_default_timeout_per_code_ms: 500,
            quote_min_timeout_ms: 250,
            quote_slow_max_requeues: 2,
            quote_hedge_after_ms: 0,
            quote_hedge_max_fraction: 0.05,
            quote_pipeline_window: 1,
            quote_tail_slow_worker_remaining_fraction: 0.0,
            quote_tail_slow_worker_fraction: 0.33,
            quote_tail_slow_worker_defer_wait_ms: 25,
            health_history_path: None,
            health_persist_interval: Duration::from_secs(5),
        }
    }
}

fn unique_server_entries(servers: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    servers
        .into_iter()
        .filter_map(|server| {
            let server = server.trim().to_string();
            if server.is_empty() || !seen.insert(server.clone()) {
                None
            } else {
                Some(server)
            }
        })
        .collect()
}

impl ManagerConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Some(value) = env_duration_ms("TDX_HEARTBEAT_INTERVAL_MS") {
            config.heartbeat_interval = value;
        }
        if let Some(value) = env_duration_ms("TDX_CONNECT_RETRY_DELAY_MS") {
            config.connect_retry_delay = value;
        }
        if let Some(value) = env_usize("TDX_SERVER_CANDIDATE_MULTIPLIER") {
            config.candidate_multiplier = value.max(1);
        }
        if let Some(value) = env_usize("TDX_FAILURE_COOLDOWN_THRESHOLD") {
            config.failure_cooldown_threshold = value.max(1);
        }
        if let Some(value) = env_duration_ms("TDX_FAILURE_COOLDOWN_MS") {
            config.failure_cooldown = value;
        }
        if let Some(value) = env_duration_ms("TDX_FAILURE_COOLDOWN_MAX_MS") {
            config.failure_cooldown_max = value;
        }
        config.failure_cooldown_max = config.failure_cooldown_max.max(config.failure_cooldown);
        if let Some(value) = env_u64("TDX_SLOW_LATENCY_THRESHOLD_MS") {
            config.slow_latency_threshold_ms = value;
        }
        if let Some(value) = env_usize("TDX_SLOW_COOLDOWN_THRESHOLD") {
            config.slow_cooldown_threshold = value.max(1);
        }
        if let Some(value) = env_duration_ms("TDX_SLOW_COOLDOWN_MS") {
            config.slow_cooldown = value;
        }
        if let Some(value) = env_f64("TDX_SERVER_FAILURE_PENALTY_MS") {
            config.failure_penalty_ms = value.max(0.0);
        }
        if let Some(value) = env_f64("TDX_SERVER_CONSECUTIVE_FAILURE_PENALTY_MS") {
            config.consecutive_failure_penalty_ms = value.max(0.0);
        }
        if let Some(value) = env_f64("TDX_SERVER_SLOW_PENALTY_MS") {
            config.slow_penalty_ms = value.max(0.0);
        }
        if let Some(value) = env_u64("TDX_STARTUP_BLACKLIST_LATENCY_THRESHOLD_MS") {
            config.startup_blacklist_latency_threshold_ms = value;
        }
        if let Some(value) = env_usize("TDX_STARTUP_BENCHMARK_QUOTE_COUNT") {
            config.startup_benchmark_quote_count = value.max(1);
        }
        if let Some(value) = env_f64("TDX_STARTUP_SPEED_FILTER_RATIO") {
            config.startup_speed_filter_ratio = value.max(0.0);
        }
        if let Some(value) = env_usize("TDX_STARTUP_SPEED_FILTER_MIN_WORKERS") {
            config.startup_speed_filter_min_workers = value;
        }
        if let Some(value) = env_u64("TDX_QUOTE_TIMEOUT_PER_CODE_MS") {
            config.quote_default_timeout_per_code_ms = value.max(1);
        }
        if let Some(value) = env_u64("TDX_QUOTE_MIN_TIMEOUT_MS") {
            config.quote_min_timeout_ms = value;
        }
        if let Some(value) = env_usize("TDX_QUOTE_SLOW_MAX_REQUEUES") {
            config.quote_slow_max_requeues = clamp_quote_slow_max_requeues(value);
        }
        if let Some(value) = env_u64("TDX_QUOTE_HEDGE_AFTER_MS") {
            config.quote_hedge_after_ms = value;
        }
        if let Some(value) = env_f64("TDX_QUOTE_HEDGE_MAX_FRACTION") {
            config.quote_hedge_max_fraction = value.clamp(0.0, 1.0);
        }
        if let Some(value) = env_usize("TDX_QUOTE_PIPELINE_WINDOW") {
            config.quote_pipeline_window = value.clamp(1, 64);
        }
        if let Some(value) = env_f64("TDX_QUOTE_TAIL_SLOW_WORKER_REMAINING_FRACTION") {
            config.quote_tail_slow_worker_remaining_fraction = value.clamp(0.0, 1.0);
        }
        if let Some(value) = env_f64("TDX_QUOTE_TAIL_SLOW_WORKER_FRACTION") {
            config.quote_tail_slow_worker_fraction = value.clamp(0.0, 0.95);
        }
        if let Some(value) = env_u64("TDX_QUOTE_TAIL_SLOW_WORKER_DEFER_WAIT_MS") {
            config.quote_tail_slow_worker_defer_wait_ms = value.max(1);
        }
        if let Some(value) = std::env::var_os("TDX_SERVER_HEALTH_HISTORY_PATH")
            && !value.is_empty()
        {
            config.health_history_path = Some(PathBuf::from(value));
        }
        if let Some(value) = env_duration_ms("TDX_SERVER_HEALTH_PERSIST_INTERVAL_MS") {
            config.health_persist_interval = value;
        }
        config
    }
}

fn clamp_quote_slow_max_requeues(value: usize) -> usize {
    value.clamp(QUOTE_SLOW_MAX_REQUEUES_MIN, QUOTE_SLOW_MAX_REQUEUES_MAX)
}

#[derive(Clone, Debug)]
struct QuoteSchedulingPolicy {
    default_timeout_per_code_ms: u64,
    min_timeout_ms: u64,
    slow_max_requeues: usize,
}

impl From<&ManagerConfig> for QuoteSchedulingPolicy {
    fn from(config: &ManagerConfig) -> Self {
        Self {
            default_timeout_per_code_ms: config.quote_default_timeout_per_code_ms,
            min_timeout_ms: config.quote_min_timeout_ms,
            slow_max_requeues: config.quote_slow_max_requeues,
        }
    }
}

impl QuoteSchedulingPolicy {
    fn deadline_for_quote_chunk(
        &self,
        quote_count: usize,
        avg_quote_ms_per_code: Option<f64>,
    ) -> Option<Duration> {
        if quote_count == 0 {
            return None;
        }
        let default_per_code = self.default_timeout_per_code_ms.max(1) as f64;
        let per_code_ms = avg_quote_ms_per_code
            .unwrap_or(default_per_code)
            .max(default_per_code)
            .max(1.0);
        let deadline_ms = (per_code_ms * quote_count as f64).ceil() as u64;
        Some(Duration::from_millis(deadline_ms.max(self.min_timeout_ms)))
    }

    fn deadline_missed(&self, elapsed: Duration, deadline: Option<Duration>) -> bool {
        deadline.is_some_and(|deadline| elapsed > deadline)
    }

    fn should_requeue_after_deadline_miss(&self, attempts: usize) -> bool {
        attempts < self.slow_max_requeues
    }

    fn quote_chunk_size(&self, requested_batch_size: usize) -> usize {
        requested_batch_size.max(1)
    }
}

#[derive(Clone)]
pub struct TdxDataManager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    initialized: AtomicBool,
    stopping: AtomicBool,
    client_mode: Mutex<ClientMode>,
    queues: Mutex<TaskQueues>,
    queue_ready: Condvar,
    batches: Mutex<HashMap<String, BatchRecord>>,
    active_quote_tasks: Mutex<HashMap<BatchTaskKey, ActiveQuoteTask>>,
    default_results: Mutex<VecDeque<TaskResult>>,
    workers: Mutex<Vec<WorkerThread>>,
    total_tasks: AtomicUsize,
    completed_tasks: AtomicUsize,
    connected_threads: AtomicUsize,
    batch_sequence: AtomicU64,
    server_health: Mutex<HashMap<String, ServerHealthState>>,
    config: Mutex<ManagerConfig>,
    last_health_persist: Mutex<Option<Instant>>,
}

#[derive(Clone, Debug, Default)]
struct ServerHealthState {
    selected: bool,
    validated: bool,
    connected: bool,
    success_count: usize,
    failure_count: usize,
    consecutive_failures: usize,
    consecutive_slow: usize,
    validation_latency_ms: Option<u64>,
    last_latency_ms: Option<u64>,
    avg_latency_ms: Option<f64>,
    avg_quote_ms_per_code: Option<f64>,
    blacklisted: bool,
    cooldown_until: Option<Instant>,
    last_error: String,
    last_no_replacement_log_at: Option<Instant>,
}

#[derive(Default)]
struct TaskQueues {
    critical: VecDeque<QueuedTask>,
    normal: VecDeque<QueuedTask>,
    background: VecDeque<QueuedTask>,
    critical_burst: usize,
}

#[derive(Clone)]
struct QueuedTask {
    batch_id: String,
    task: Task,
    counts_for_queue_drain: bool,
    attempts: usize,
}

struct BatchRecord {
    total: usize,
    completed: usize,
    results: Vec<TaskResult>,
    completed_task_ids: HashSet<String>,
    hedge_enqueued_task_ids: HashSet<String>,
    handle: Arc<BatchHandle>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BatchTaskKey {
    batch_id: String,
    task_id: String,
}

#[derive(Clone)]
struct ActiveQuoteTask {
    batch_id: String,
    task: Task,
    server: String,
    started_at: Instant,
}

struct WorkerThread {
    pending: Arc<AtomicUsize>,
    activity: Arc<Mutex<WorkerActivityState>>,
    handle: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct WorkerActivityState {
    server: String,
    connected: bool,
    active_batch_id: String,
    active_task_id: String,
    active_task_type: Option<TaskType>,
    active_quote_count: usize,
    active_started_at: Option<Instant>,
}

#[derive(Debug)]
struct StartupValidationResult {
    server: String,
    success: bool,
    latency_ms: Option<u64>,
    threshold_units: usize,
    error: String,
}

const CRITICAL_BURST_BEFORE_NORMAL: usize = 6;
const NO_REPLACEMENT_LOG_INTERVAL: Duration = Duration::from_secs(15);
const QUOTE_SLOW_MAX_REQUEUES_MIN: usize = 1;
const QUOTE_SLOW_MAX_REQUEUES_MAX: usize = 3;

impl Default for TdxDataManager {
    fn default() -> Self {
        Self {
            inner: Arc::new(ManagerInner {
                initialized: AtomicBool::new(false),
                stopping: AtomicBool::new(false),
                client_mode: Mutex::new(ClientMode::Mock),
                queues: Mutex::new(TaskQueues::default()),
                queue_ready: Condvar::new(),
                batches: Mutex::new(HashMap::new()),
                active_quote_tasks: Mutex::new(HashMap::new()),
                default_results: Mutex::new(VecDeque::new()),
                workers: Mutex::new(Vec::new()),
                total_tasks: AtomicUsize::new(0),
                completed_tasks: AtomicUsize::new(0),
                connected_threads: AtomicUsize::new(0),
                batch_sequence: AtomicU64::new(1),
                server_health: Mutex::new(HashMap::new()),
                config: Mutex::new(ManagerConfig::from_env()),
                last_health_persist: Mutex::new(None),
            }),
        }
    }
}

impl TdxDataManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_client_mode(&self, client_mode: ClientMode) {
        let mut mode = self.inner.client_mode.lock().unwrap();
        *mode = client_mode;
    }

    pub fn client_mode(&self) -> ClientMode {
        *self.inner.client_mode.lock().unwrap()
    }

    pub fn set_config(&self, config: ManagerConfig) {
        let mut current = self.inner.config.lock().unwrap();
        *current = config;
    }

    pub fn config(&self) -> ManagerConfig {
        self.inner.config.lock().unwrap().clone()
    }

    pub fn initialize(
        &self,
        servers: &[String],
        stock_codes: &[String],
        desired_workers: usize,
    ) -> bool {
        if self.inner.initialized.swap(true, Ordering::SeqCst) {
            return true;
        }

        self.inner.stopping.store(false, Ordering::SeqCst);
        let servers = unique_server_entries(if servers.is_empty() {
            pick_servers(10)
        } else {
            servers.to_vec()
        });
        seed_server_health(&self.inner, &servers);
        load_server_health_history(&self.inner);

        let mut workers = self.inner.workers.lock().unwrap();
        let client_mode = self.client_mode();
        let selected_servers = unique_server_entries(select_servers_for_startup(
            &self.inner,
            &servers,
            stock_codes,
            desired_workers.max(1),
            client_mode,
        ));
        for server in selected_servers {
            let inner = self.inner.clone();
            let pending = Arc::new(AtomicUsize::new(0));
            let pending_worker = pending.clone();
            let server_clone = server.clone();
            let activity = Arc::new(Mutex::new(WorkerActivityState {
                server: server.clone(),
                ..WorkerActivityState::default()
            }));
            let activity_worker = activity.clone();
            let handle = thread::Builder::new()
                .name(format!("tdx-worker-{server_clone}"))
                .spawn(move || {
                    worker_loop(
                        inner,
                        server_clone,
                        pending_worker,
                        activity_worker,
                        client_mode,
                    )
                })
                .ok();
            workers.push(WorkerThread {
                pending,
                activity,
                handle,
            });
        }
        true
    }

    pub fn shutdown(&self) {
        if !self.inner.initialized.swap(false, Ordering::SeqCst) {
            return;
        }

        self.inner.stopping.store(true, Ordering::SeqCst);
        {
            let mut queues = self.inner.queues.lock().unwrap();
            queues.critical.clear();
            queues.normal.clear();
            queues.background.clear();
        }
        self.inner.queue_ready.notify_all();

        {
            let batches = self.inner.batches.lock().unwrap();
            for batch in batches.values() {
                batch.handle.cancel();
            }
        }

        let mut join_handles = Vec::new();
        {
            let mut workers = self.inner.workers.lock().unwrap();
            for worker in workers.iter_mut() {
                if let Some(handle) = worker.handle.take() {
                    join_handles.push(handle);
                }
            }
            workers.clear();
        }

        for handle in join_handles {
            let _ = handle.join();
        }

        persist_server_health(&self.inner, true);
    }

    pub fn schedule_tasks(
        &self,
        stock_tasks: &BTreeMap<String, usize>,
        kline_type: KlineType,
    ) -> bool {
        self.schedule_batch(stock_tasks, kline_type, self.next_batch_id("default"))
    }

    pub fn schedule_batch(
        &self,
        stock_tasks: &BTreeMap<String, usize>,
        kline_type: KlineType,
        batch_id: impl Into<String>,
    ) -> bool {
        let batch_id = batch_id.into();
        let tasks = stock_tasks
            .iter()
            .map(|(code, count)| Task::kline(code.clone(), *count, kline_type))
            .collect::<Vec<_>>();
        self.schedule_task_batch(batch_id, tasks, TaskType::Kline, kline_type)
    }

    pub fn schedule_quote_tasks(&self, stocks: &[(u8, String)], batch_size: usize) -> bool {
        self.schedule_quote_batch(stocks, batch_size, self.next_batch_id("quote"))
    }

    pub fn schedule_quote_batch(
        &self,
        stocks: &[(u8, String)],
        batch_size: usize,
        batch_id: impl Into<String>,
    ) -> bool {
        let batch_id = batch_id.into();
        let config = current_config(&self.inner);
        let policy = QuoteSchedulingPolicy::from(&config);
        let batch_size = policy.quote_chunk_size(batch_size);
        let tasks = stocks
            .chunks(batch_size)
            .enumerate()
            .map(|(index, chunk)| {
                Task::quote(chunk.to_vec()).with_task_id(format!("{batch_id}_chunk_{index:04}"))
            })
            .collect::<Vec<_>>();
        self.schedule_task_batch(batch_id, tasks, TaskType::Quote, KlineType::KlineDaily)
    }

    pub fn schedule_generic_batch(&self, tasks: &[Task], batch_id: impl Into<String>) -> bool {
        let batch_id = batch_id.into();
        let task_type = tasks
            .first()
            .map(|value| value.task_type)
            .unwrap_or(TaskType::Kline);
        self.schedule_task_batch(batch_id, tasks.to_vec(), task_type, KlineType::KlineDaily)
    }

    pub fn start_kline_batch(
        &self,
        stock_tasks: &BTreeMap<String, usize>,
        kline_type: KlineType,
    ) -> Arc<BatchHandle> {
        let batch_id = self.next_batch_id("kline");
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), stock_tasks.len()));
        self.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            stock_tasks.len(),
            TaskType::Kline,
            kline_type,
        );
        let tasks = stock_tasks
            .iter()
            .map(|(code, count)| Task::kline(code.clone(), *count, kline_type))
            .collect::<Vec<_>>();
        self.enqueue_task_batch(batch_id, tasks);
        handle
    }

    pub fn start_quote_batch(
        &self,
        stocks: &[(u8, String)],
        batch_size: usize,
    ) -> Arc<BatchHandle> {
        let batch_id = self.next_batch_id("quote");
        let config = current_config(&self.inner);
        let policy = QuoteSchedulingPolicy::from(&config);
        let batch_size = policy.quote_chunk_size(batch_size);
        let chunks = stocks.chunks(batch_size).collect::<Vec<_>>();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), chunks.len()));
        self.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            chunks.len(),
            TaskType::Quote,
            KlineType::KlineDaily,
        );
        let tasks = chunks
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| {
                Task::quote(chunk.to_vec()).with_task_id(format!("{batch_id}_chunk_{index:04}"))
            })
            .collect::<Vec<_>>();
        self.enqueue_task_batch(batch_id, tasks);
        handle
    }

    pub fn start_generic_batch(&self, tasks: &[Task]) -> Arc<BatchHandle> {
        let batch_id = self.next_batch_id("generic");
        let task_type = tasks
            .first()
            .map(|value| value.task_type)
            .unwrap_or(TaskType::Kline);
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), tasks.len()));
        self.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            tasks.len(),
            task_type,
            KlineType::KlineDaily,
        );
        self.enqueue_task_batch(batch_id, tasks.to_vec());
        handle
    }

    pub fn cancel_batch(&self, batch_id: &str) {
        {
            let mut queues = self.inner.queues.lock().unwrap();
            queues.critical.retain(|task| task.batch_id != batch_id);
            queues.normal.retain(|task| task.batch_id != batch_id);
            queues.background.retain(|task| task.batch_id != batch_id);
        }
        remove_active_quote_tasks_for_batch(&self.inner, batch_id);

        let batches = self.inner.batches.lock().unwrap();
        if let Some(batch) = batches.get(batch_id) {
            batch.handle.cancel();
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.inner.initialized.load(Ordering::SeqCst)
    }

    pub fn get_total_thread_count(&self) -> usize {
        let workers = self.inner.workers.lock().unwrap();
        workers.len()
    }

    pub fn get_connected_thread_count(&self) -> usize {
        self.inner.connected_threads.load(Ordering::SeqCst)
    }

    pub fn get_pending_task_count(&self) -> usize {
        let queues = self.inner.queues.lock().unwrap();
        let queued = queues.critical.len() + queues.normal.len() + queues.background.len();
        let workers = self.inner.workers.lock().unwrap();
        let active = workers
            .iter()
            .map(|worker| worker.pending.load(Ordering::SeqCst))
            .sum::<usize>();
        queued + active
    }

    pub fn get_statistics(&self) -> ManagerStatistics {
        let batch_count = self.inner.batches.lock().unwrap().len();
        ManagerStatistics {
            total_tasks: self.inner.total_tasks.load(Ordering::SeqCst),
            completed_tasks: self.inner.completed_tasks.load(Ordering::SeqCst),
            connected_threads: self.inner.connected_threads.load(Ordering::SeqCst),
            pending_tasks: self.get_pending_task_count(),
            batch_count,
        }
    }

    pub fn get_worker_activity(&self) -> Vec<WorkerActivitySnapshot> {
        let now = Instant::now();
        let workers = self.inner.workers.lock().unwrap();
        let mut snapshots = workers
            .iter()
            .map(|worker| {
                let activity = worker.activity.lock().unwrap();
                let active = activity.active_started_at.is_some();
                WorkerActivitySnapshot {
                    server: activity.server.clone(),
                    connected: activity.connected,
                    active,
                    active_batch_id: active.then(|| activity.active_batch_id.clone()),
                    active_task_id: active.then(|| activity.active_task_id.clone()),
                    active_task_type: active.then_some(activity.active_task_type).flatten(),
                    active_quote_count: if active {
                        activity.active_quote_count
                    } else {
                        0
                    },
                    active_elapsed_ms: activity.active_started_at.map(|started_at| {
                        now.duration_since(started_at)
                            .as_millis()
                            .min(u128::from(u64::MAX)) as u64
                    }),
                }
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            right
                .active
                .cmp(&left.active)
                .then_with(|| right.connected.cmp(&left.connected))
                .then_with(|| {
                    right
                        .active_elapsed_ms
                        .unwrap_or(0)
                        .cmp(&left.active_elapsed_ms.unwrap_or(0))
                })
                .then_with(|| left.server.cmp(&right.server))
        });
        snapshots
    }

    pub fn get_server_health(&self) -> Vec<ServerHealthSnapshot> {
        let now = Instant::now();
        let health = self.inner.server_health.lock().unwrap();
        let mut snapshots = health
            .iter()
            .map(|(server, state)| ServerHealthSnapshot {
                server: server.clone(),
                selected: state.selected,
                validated: state.validated,
                connected: state.connected,
                success_count: state.success_count,
                failure_count: state.failure_count,
                consecutive_failures: state.consecutive_failures,
                consecutive_slow: state.consecutive_slow,
                validation_latency_ms: state.validation_latency_ms,
                last_latency_ms: state.last_latency_ms,
                avg_latency_ms: state.avg_latency_ms,
                avg_quote_ms_per_code: state.avg_quote_ms_per_code,
                blacklisted: state.blacklisted,
                cooldown_remaining_ms: state
                    .cooldown_until
                    .and_then(|deadline| deadline.checked_duration_since(now))
                    .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64)
                    .unwrap_or(0),
                last_error: state.last_error.clone(),
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            right
                .selected
                .cmp(&left.selected)
                .then_with(|| right.connected.cmp(&left.connected))
                .then_with(|| right.validated.cmp(&left.validated))
                .then_with(|| {
                    left.validation_latency_ms
                        .unwrap_or(u64::MAX)
                        .cmp(&right.validation_latency_ms.unwrap_or(u64::MAX))
                })
                .then_with(|| left.server.cmp(&right.server))
        });
        snapshots
    }

    pub fn get_completed_results(&self) -> Vec<TaskResult> {
        self.get_completed_results_limit(usize::MAX)
    }

    pub fn get_completed_results_limit(&self, limit: usize) -> Vec<TaskResult> {
        let mut queue = self.inner.default_results.lock().unwrap();
        let count = queue.len().min(limit);
        let mut results = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(result) = queue.pop_front() {
                results.push(result);
            }
        }
        results
    }

    pub fn get_batch_results(&self, batch_id: &str) -> Vec<TaskResult> {
        let batches = self.inner.batches.lock().unwrap();
        batches
            .get(batch_id)
            .map(|batch| batch.results.clone())
            .unwrap_or_default()
    }

    pub fn take_batch_results(&self, batch_id: &str) -> Vec<TaskResult> {
        remove_active_quote_tasks_for_batch(&self.inner, batch_id);
        let mut batches = self.inner.batches.lock().unwrap();
        batches
            .remove(batch_id)
            .map(|mut batch| std::mem::take(&mut batch.results))
            .unwrap_or_default()
    }

    pub fn drop_batch(&self, batch_id: &str) -> bool {
        remove_active_quote_tasks_for_batch(&self.inner, batch_id);
        let mut batches = self.inner.batches.lock().unwrap();
        batches.remove(batch_id).is_some()
    }

    pub fn get_batch_progress(&self, batch_id: &str) -> (usize, usize) {
        let batches = self.inner.batches.lock().unwrap();
        batches
            .get(batch_id)
            .map(|batch| (batch.completed, batch.total))
            .unwrap_or((0, 0))
    }

    pub fn get_batch_queue_progress(&self, batch_id: &str) -> (usize, usize) {
        let batches = self.inner.batches.lock().unwrap();
        batches
            .get(batch_id)
            .map(|batch| (batch.handle.queued_count(), batch.total))
            .unwrap_or((0, 0))
    }

    pub fn wait_for_all_tasks(&self, timeout: Duration) -> bool {
        let handles = {
            let batches = self.inner.batches.lock().unwrap();
            batches
                .values()
                .map(|batch| batch.handle.clone())
                .collect::<Vec<_>>()
        };

        let deadline = Instant::now() + timeout;
        for handle in handles {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            if !handle.wait_produced(Some(deadline - now)) {
                return false;
            }
        }
        true
    }

    pub fn wait_for_batch(&self, batch_id: &str, timeout: Duration) -> bool {
        let handle = {
            let batches = self.inner.batches.lock().unwrap();
            batches.get(batch_id).map(|batch| batch.handle.clone())
        };
        handle.is_some_and(|handle| handle.wait_produced(Some(timeout)))
    }

    pub fn wait_for_batch_queue_drained(&self, batch_id: &str, timeout: Duration) -> bool {
        let handle = {
            let batches = self.inner.batches.lock().unwrap();
            batches.get(batch_id).map(|batch| batch.handle.clone())
        };
        handle.is_some_and(|handle| handle.wait_queue_drained(Some(timeout)))
    }

    pub fn batch_handle(&self, batch_id: &str) -> Option<Arc<BatchHandle>> {
        let batches = self.inner.batches.lock().unwrap();
        batches.get(batch_id).map(|batch| batch.handle.clone())
    }

    fn schedule_task_batch(
        &self,
        batch_id: String,
        tasks: Vec<Task>,
        task_type: TaskType,
        kline_type: KlineType,
    ) -> bool {
        if tasks.is_empty() {
            return false;
        }
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), tasks.len()));
        self.insert_batch_handle(batch_id.clone(), handle, tasks.len(), task_type, kline_type);
        self.enqueue_task_batch(batch_id, tasks);
        true
    }

    fn insert_batch_handle(
        &self,
        batch_id: String,
        handle: Arc<BatchHandle>,
        total: usize,
        _task_type: TaskType,
        _kline_type: KlineType,
    ) {
        self.inner.total_tasks.fetch_add(total, Ordering::SeqCst);
        let mut batches = self.inner.batches.lock().unwrap();
        batches.insert(
            batch_id,
            BatchRecord {
                total,
                completed: 0,
                results: Vec::with_capacity(total),
                completed_task_ids: HashSet::with_capacity(total),
                hedge_enqueued_task_ids: HashSet::new(),
                handle,
            },
        );
    }

    fn enqueue_task_batch(&self, batch_id: String, tasks: Vec<Task>) {
        let mut queues = self.inner.queues.lock().unwrap();
        for task in tasks {
            let priority = task.priority;
            let queued = QueuedTask {
                batch_id: batch_id.clone(),
                task,
                counts_for_queue_drain: true,
                attempts: 0,
            };
            let lane = lane_for_priority_mut(&mut queues, priority);
            push_task_into_lane(lane, queued, false);
        }
        self.inner.queue_ready.notify_all();
    }

    fn next_batch_id(&self, prefix: &str) -> String {
        let value = self.inner.batch_sequence.fetch_add(1, Ordering::SeqCst);
        format!("{prefix}_{value:08}")
    }
}

impl Drop for TdxDataManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_loop(
    inner: Arc<ManagerInner>,
    server: String,
    pending: Arc<AtomicUsize>,
    activity: Arc<Mutex<WorkerActivityState>>,
    client_mode: ClientMode,
) {
    let mut current_server = server;
    let mut api = build_api(client_mode, &current_server);
    let mut connected = connect_worker(&inner, &api, &current_server);
    update_worker_connection(&activity, &current_server, connected);
    if connected {
        inner.connected_threads.fetch_add(1, Ordering::SeqCst);
    }
    let mut last_heartbeat = Instant::now();

    loop {
        if inner.stopping.load(Ordering::SeqCst) {
            break;
        }
        let config = current_config(&inner);
        if let Some(wait) = server_cooldown_remaining(&inner, &current_server) {
            if connected {
                api.disconnect();
                inner.connected_threads.fetch_sub(1, Ordering::SeqCst);
                mark_connected(&inner, &current_server, false);
                connected = false;
                update_worker_connection(&activity, &current_server, connected);
            }
            if switch_worker_server(
                &inner,
                &mut current_server,
                &mut api,
                &mut connected,
                client_mode,
            ) {
                update_worker_connection(&activity, &current_server, connected);
                last_heartbeat = Instant::now();
            } else {
                thread::sleep(wait.min(Duration::from_millis(500)));
            }
            continue;
        }
        if server_is_blacklisted(&inner, &current_server) {
            if connected {
                api.disconnect();
                inner.connected_threads.fetch_sub(1, Ordering::SeqCst);
                mark_connected(&inner, &current_server, false);
                connected = false;
                update_worker_connection(&activity, &current_server, connected);
            }
            if switch_worker_server(
                &inner,
                &mut current_server,
                &mut api,
                &mut connected,
                client_mode,
            ) {
                update_worker_connection(&activity, &current_server, connected);
                last_heartbeat = Instant::now();
            } else {
                thread::sleep(config.connect_retry_delay);
            }
            continue;
        }
        if !connected {
            connected = connect_worker(&inner, &api, &current_server);
            if connected {
                inner.connected_threads.fetch_add(1, Ordering::SeqCst);
                update_worker_connection(&activity, &current_server, connected);
                last_heartbeat = Instant::now();
            } else if switch_worker_server(
                &inner,
                &mut current_server,
                &mut api,
                &mut connected,
                client_mode,
            ) {
                update_worker_connection(&activity, &current_server, connected);
                last_heartbeat = Instant::now();
            } else {
                thread::sleep(config.connect_retry_delay);
            }
            continue;
        }
        let wait_timeout = if connected {
            config
                .heartbeat_interval
                .checked_sub(last_heartbeat.elapsed())
                .unwrap_or_else(|| Duration::from_millis(0))
        } else {
            config.connect_retry_delay
        };
        let wait_timeout = next_quote_hedge_due_in(&inner)
            .map(|hedge_wait| wait_timeout.min(hedge_wait))
            .unwrap_or(wait_timeout);

        let queued = match take_next_task(&inner, &current_server, Some(wait_timeout)) {
            WorkerPoll::Task(task) => task,
            WorkerPoll::Stopped => break,
            WorkerPoll::Timeout => {
                if connected && last_heartbeat.elapsed() >= config.heartbeat_interval {
                    if api.heartbeat().is_ok() {
                        last_heartbeat = Instant::now();
                    } else {
                        api.disconnect();
                        inner.connected_threads.fetch_sub(1, Ordering::SeqCst);
                        record_connection_failure(&inner, &current_server, "heartbeat failed");
                        connected = false;
                        update_worker_connection(&activity, &current_server, connected);
                        if switch_worker_server(
                            &inner,
                            &mut current_server,
                            &mut api,
                            &mut connected,
                            client_mode,
                        ) {
                            update_worker_connection(&activity, &current_server, connected);
                            last_heartbeat = Instant::now();
                        }
                    }
                } else if !connected {
                    connected = connect_worker(&inner, &api, &current_server);
                    if connected {
                        inner.connected_threads.fetch_add(1, Ordering::SeqCst);
                        update_worker_connection(&activity, &current_server, connected);
                        last_heartbeat = Instant::now();
                    } else if switch_worker_server(
                        &inner,
                        &mut current_server,
                        &mut api,
                        &mut connected,
                        client_mode,
                    ) {
                        update_worker_connection(&activity, &current_server, connected);
                        last_heartbeat = Instant::now();
                    }
                }
                continue;
            }
        };

        if let Some(wait) = server_cooldown_remaining(&inner, &current_server) {
            if switch_worker_server(
                &inner,
                &mut current_server,
                &mut api,
                &mut connected,
                client_mode,
            ) {
                update_worker_connection(&activity, &current_server, connected);
                last_heartbeat = Instant::now();
            } else {
                pending.store(0, Ordering::SeqCst);
                clear_worker_active_task(&activity);
                requeue_task(&inner, queued.clone(), true);
                thread::sleep(wait.min(Duration::from_millis(500)));
                continue;
            }
        }

        if server_is_blacklisted(&inner, &current_server) {
            if switch_worker_server(
                &inner,
                &mut current_server,
                &mut api,
                &mut connected,
                client_mode,
            ) {
                update_worker_connection(&activity, &current_server, connected);
                last_heartbeat = Instant::now();
            } else {
                pending.store(0, Ordering::SeqCst);
                clear_worker_active_task(&activity);
                deliver_result(
                    &inner,
                    &queued.batch_id,
                    TaskResult::failure(
                        &queued.task,
                        format!(
                            "server {} is blacklisted and no replacement is available",
                            current_server
                        ),
                        current_server.clone(),
                        Duration::ZERO,
                    ),
                );
                continue;
            }
        }

        if !connected {
            connected = connect_worker(&inner, &api, &current_server);
            if connected {
                inner.connected_threads.fetch_add(1, Ordering::SeqCst);
                update_worker_connection(&activity, &current_server, connected);
                last_heartbeat = Instant::now();
            } else {
                if switch_worker_server(
                    &inner,
                    &mut current_server,
                    &mut api,
                    &mut connected,
                    client_mode,
                ) {
                    update_worker_connection(&activity, &current_server, connected);
                    last_heartbeat = Instant::now();
                } else {
                    pending.store(0, Ordering::SeqCst);
                    clear_worker_active_task(&activity);
                    requeue_task(&inner, queued.clone(), false);
                    thread::sleep(config.connect_retry_delay);
                    continue;
                }
            }
        }

        let queued_batch = collect_quote_pipeline_batch(
            &inner,
            &current_server,
            queued,
            config.quote_pipeline_window,
        );

        pending.store(1, Ordering::SeqCst);
        if let Some(first) = queued_batch.first() {
            mark_worker_active_task(&activity, &first.batch_id, &first.task);
        }
        for queued in &queued_batch {
            mark_quote_task_started(&inner, &queued.batch_id, &queued.task, &current_server);
        }
        let deadlines = queued_batch
            .iter()
            .map(|queued| quote_task_deadline(&inner, &current_server, &queued.task))
            .collect::<Vec<_>>();
        let quote_request_started_unix_ms = queued_batch
            .first()
            .is_some_and(|queued| queued.task.task_type == TaskType::Quote)
            .then(unix_time_ms);
        let mut results =
            if queued_batch.len() > 1 {
                let tasks = queued_batch
                    .iter()
                    .map(|queued| queued.task.clone())
                    .collect::<Vec<_>>();
                api.execute_quote_pipeline(&tasks)
            } else {
                let queued = queued_batch
                    .first()
                    .expect("worker loop always has at least one task");
                vec![api.execute_task_with_read_timeout(
                    &queued.task,
                    deadlines.first().copied().flatten(),
                )]
            };
        if let Some(request_started_unix_ms) = quote_request_started_unix_ms {
            annotate_quote_request_timing(
                &mut results,
                queued_batch
                    .iter()
                    .map(|queued| queued.task.quote_stocks.len()),
                request_started_unix_ms,
                unix_time_ms(),
            );
        }
        pending.store(0, Ordering::SeqCst);
        clear_worker_active_task(&activity);

        let mut quote_requeued = false;
        for ((queued, result), deadline) in queued_batch.into_iter().zip(results).zip(deadlines) {
            if handle_runtime_empty_quote_response(&inner, &current_server, queued.clone(), &result)
            {
                quote_requeued = true;
                continue;
            }
            if let Some(deadline) = deadline
                && handle_runtime_quote_deadline_miss(
                    &inner,
                    &current_server,
                    queued.clone(),
                    &result,
                    deadline,
                )
            {
                quote_requeued = true;
                continue;
            }
            record_task_result(&inner, &current_server, &result);
            deliver_result(&inner, &queued.batch_id, result);
        }

        if quote_requeued {
            if connected {
                api.disconnect();
                inner.connected_threads.fetch_sub(1, Ordering::SeqCst);
                connected = false;
                update_worker_connection(&activity, &current_server, connected);
            }
            if switch_worker_server(
                &inner,
                &mut current_server,
                &mut api,
                &mut connected,
                client_mode,
            ) {
                update_worker_connection(&activity, &current_server, connected);
                last_heartbeat = Instant::now();
            }
            continue;
        }

        let now_connected = api.is_connected();
        if now_connected && !connected {
            inner.connected_threads.fetch_add(1, Ordering::SeqCst);
            last_heartbeat = Instant::now();
        } else if !now_connected && connected {
            inner.connected_threads.fetch_sub(1, Ordering::SeqCst);
        }
        connected = now_connected;
        update_worker_connection(&activity, &current_server, connected);
    }

    api.disconnect();
    if connected {
        inner.connected_threads.fetch_sub(1, Ordering::SeqCst);
    }
    update_worker_connection(&activity, &current_server, false);
    clear_worker_active_task(&activity);
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn annotate_quote_request_timing<I>(
    results: &mut [TaskResult],
    request_code_counts: I,
    request_started_unix_ms: u64,
    response_completed_unix_ms: u64,
) where
    I: IntoIterator<Item = usize>,
{
    for result in results.iter_mut() {
        result.metadata.insert(
            "quote_request_started_unix_ms".to_string(),
            request_started_unix_ms.to_string(),
        );
        result.metadata.insert(
            "quote_response_completed_unix_ms".to_string(),
            response_completed_unix_ms.to_string(),
        );
    }
    for (result, request_code_count) in results.iter_mut().zip(request_code_counts) {
        result.metadata.insert(
            "quote_request_code_count".to_string(),
            request_code_count.to_string(),
        );
    }
}

fn collect_quote_pipeline_batch(
    inner: &Arc<ManagerInner>,
    current_server: &str,
    first: QueuedTask,
    max_window: usize,
) -> Vec<QueuedTask> {
    if max_window <= 1
        || first.task.task_type != TaskType::Quote
        || tail_phase_slow_worker(inner, current_server, &first)
    {
        return vec![first];
    }

    let mut batch = vec![first];
    let mut queues = inner.queues.lock().unwrap();
    while batch.len() < max_window {
        if queued_task_count(&queues) <= idle_connected_worker_count(inner, current_server) {
            break;
        }
        let Some(next) = peek_next_task_from_queues(&queues) else {
            break;
        };
        if next.task.task_type != TaskType::Quote
            || tail_phase_slow_worker(inner, current_server, next)
        {
            break;
        }
        let Some(next) = take_next_task_from_queues(&mut queues) else {
            break;
        };
        if next.counts_for_queue_drain
            && let Some(handle) = batch_handle(inner, &next.batch_id)
        {
            handle.mark_dequeued_for_execution();
        }
        batch.push(next);
    }
    batch
}

fn queued_task_count(queues: &TaskQueues) -> usize {
    queues.critical.len() + queues.normal.len() + queues.background.len()
}

fn idle_connected_worker_count(inner: &Arc<ManagerInner>, current_server: &str) -> usize {
    let workers = inner.workers.lock().unwrap();
    workers
        .iter()
        .filter(|worker| {
            let activity = worker.activity.lock().unwrap();
            activity.connected
                && activity.active_started_at.is_none()
                && activity.server != current_server
        })
        .count()
}

fn batch_handle(inner: &Arc<ManagerInner>, batch_id: &str) -> Option<Arc<BatchHandle>> {
    let batches = inner.batches.lock().unwrap();
    batches.get(batch_id).map(|batch| batch.handle.clone())
}

fn tail_phase_slow_worker(
    inner: &Arc<ManagerInner>,
    current_server: &str,
    queued: &QueuedTask,
) -> bool {
    let config = current_config(inner);
    if config.quote_tail_slow_worker_remaining_fraction <= 0.0
        || config.quote_tail_slow_worker_fraction <= 0.0
        || queued.task.task_type != TaskType::Quote
    {
        return false;
    }
    if !quote_batch_in_tail_phase(
        inner,
        &queued.batch_id,
        config.quote_tail_slow_worker_remaining_fraction,
    ) {
        return false;
    }
    server_is_in_slow_tail(
        inner,
        current_server,
        config.quote_tail_slow_worker_fraction,
    )
}

#[allow(clippy::large_enum_variant)]
enum WorkerPoll {
    Task(QueuedTask),
    Timeout,
    Stopped,
}

fn build_api(client_mode: ClientMode, server: &str) -> TdxHqApi {
    match client_mode {
        ClientMode::Mock => TdxHqApi::mock(server.to_string()),
        ClientMode::Real => TdxHqApi::real(server.to_string()),
    }
}

fn update_worker_connection(
    activity: &Arc<Mutex<WorkerActivityState>>,
    server: &str,
    connected: bool,
) {
    let mut state = activity.lock().unwrap();
    state.server = server.to_string();
    state.connected = connected;
}

fn mark_worker_active_task(
    activity: &Arc<Mutex<WorkerActivityState>>,
    batch_id: &str,
    task: &Task,
) {
    let mut state = activity.lock().unwrap();
    state.active_batch_id = batch_id.to_string();
    state.active_task_id = task.key();
    state.active_task_type = Some(task.task_type);
    state.active_quote_count = task.quote_stocks.len();
    state.active_started_at = Some(Instant::now());
}

fn clear_worker_active_task(activity: &Arc<Mutex<WorkerActivityState>>) {
    let mut state = activity.lock().unwrap();
    state.active_batch_id.clear();
    state.active_task_id.clear();
    state.active_task_type = None;
    state.active_quote_count = 0;
    state.active_started_at = None;
}

fn connect_worker(inner: &Arc<ManagerInner>, api: &TdxHqApi, server: &str) -> bool {
    match api.connect_to_server() {
        Ok(()) => {
            mark_connected(inner, server, true);
            true
        }
        Err(error) => {
            record_connection_failure(inner, server, &error);
            false
        }
    }
}

fn switch_worker_server(
    inner: &Arc<ManagerInner>,
    current_server: &mut String,
    api: &mut TdxHqApi,
    connected: &mut bool,
    client_mode: ClientMode,
) -> bool {
    let Some(next_server) = claim_replacement_server(inner, current_server) else {
        if should_log_no_replacement_server(inner, current_server) {
            log::warn!(
                "tdx worker has no replacement server current_server={} connected={}",
                current_server,
                connected
            );
        }
        return false;
    };

    if *connected {
        api.disconnect();
        inner.connected_threads.fetch_sub(1, Ordering::SeqCst);
        *connected = false;
    }

    mark_connected(inner, current_server, false);
    switch_selected_server(inner, current_server, &next_server);

    let next_api = build_api(client_mode, &next_server);
    let next_connected = connect_worker(inner, &next_api, &next_server);
    if next_connected {
        inner.connected_threads.fetch_add(1, Ordering::SeqCst);
    }

    *api = next_api;
    log::warn!(
        "tdx worker switched server old_server={} new_server={} next_connected={}",
        current_server,
        next_server,
        next_connected
    );
    *current_server = next_server;
    *connected = next_connected;
    true
}

fn should_log_no_replacement_server(inner: &Arc<ManagerInner>, server: &str) -> bool {
    let now = Instant::now();
    let mut health = inner.server_health.lock().unwrap();
    let state = health.entry(server.to_string()).or_default();
    if state
        .last_no_replacement_log_at
        .is_some_and(|last| now.duration_since(last) < NO_REPLACEMENT_LOG_INTERVAL)
    {
        return false;
    }
    state.last_no_replacement_log_at = Some(now);
    true
}

fn take_next_task(
    inner: &Arc<ManagerInner>,
    current_server: &str,
    timeout: Option<Duration>,
) -> WorkerPoll {
    let mut queues = inner.queues.lock().unwrap();
    loop {
        match select_next_task_for_worker(inner, current_server, &mut queues) {
            TaskSelection::Task(task) => {
                let handle = {
                    let batches = inner.batches.lock().unwrap();
                    batches
                        .get(&task.batch_id)
                        .map(|batch| batch.handle.clone())
                };
                if task.counts_for_queue_drain
                    && let Some(handle) = handle
                {
                    handle.mark_dequeued_for_execution();
                }
                return WorkerPoll::Task(task);
            }
            TaskSelection::Deferred(wait) => {
                if inner.stopping.load(Ordering::SeqCst) {
                    return WorkerPoll::Stopped;
                }
                inner.queue_ready.notify_all();
                let wait_timeout = timeout.map(|timeout| timeout.min(wait)).unwrap_or(wait);
                let (next_queues, outcome) = inner
                    .queue_ready
                    .wait_timeout(queues, wait_timeout)
                    .unwrap();
                queues = next_queues;
                if outcome.timed_out() {
                    return WorkerPoll::Timeout;
                }
                continue;
            }
            TaskSelection::Empty => {}
        }
        if enqueue_due_quote_hedges(inner, &mut queues) > 0 {
            continue;
        }
        if inner.stopping.load(Ordering::SeqCst) {
            return WorkerPoll::Stopped;
        }
        if let Some(wait_timeout) = timeout {
            let (next_queues, outcome) = inner
                .queue_ready
                .wait_timeout(queues, wait_timeout)
                .unwrap();
            queues = next_queues;
            if outcome.timed_out() {
                return WorkerPoll::Timeout;
            }
        } else {
            queues = inner.queue_ready.wait(queues).unwrap();
        }
    }
}

enum TaskSelection {
    Task(QueuedTask),
    Deferred(Duration),
    Empty,
}

fn select_next_task_for_worker(
    inner: &Arc<ManagerInner>,
    current_server: &str,
    queues: &mut TaskQueues,
) -> TaskSelection {
    if let Some(task) = peek_next_task_from_queues(queues)
        && should_defer_tail_quote_task(inner, current_server, task)
    {
        let wait_ms = current_config(inner).quote_tail_slow_worker_defer_wait_ms;
        return TaskSelection::Deferred(Duration::from_millis(wait_ms));
    }
    take_next_task_from_queues(queues)
        .map(TaskSelection::Task)
        .unwrap_or(TaskSelection::Empty)
}

fn peek_next_task_from_queues(queues: &TaskQueues) -> Option<&QueuedTask> {
    if !queues.critical.is_empty() {
        if !queues.normal.is_empty() && queues.critical_burst >= CRITICAL_BURST_BEFORE_NORMAL {
            return queues.normal.front();
        }
        return queues.critical.front();
    }

    queues.normal.front().or_else(|| queues.background.front())
}

fn take_next_task_from_queues(queues: &mut TaskQueues) -> Option<QueuedTask> {
    if !queues.critical.is_empty() {
        if !queues.normal.is_empty() && queues.critical_burst >= CRITICAL_BURST_BEFORE_NORMAL {
            queues.critical_burst = 0;
            return queues.normal.pop_front();
        }
        queues.critical_burst = queues.critical_burst.saturating_add(1);
        return queues.critical.pop_front();
    }

    queues.critical_burst = 0;
    if let Some(task) = queues.normal.pop_front() {
        return Some(task);
    }
    queues.background.pop_front()
}

fn should_defer_tail_quote_task(
    inner: &Arc<ManagerInner>,
    current_server: &str,
    queued: &QueuedTask,
) -> bool {
    let config = current_config(inner);
    if config.quote_tail_slow_worker_remaining_fraction <= 0.0
        || config.quote_tail_slow_worker_fraction <= 0.0
        || queued.task.task_type != TaskType::Quote
    {
        return false;
    }

    if !quote_batch_in_tail_phase(
        inner,
        &queued.batch_id,
        config.quote_tail_slow_worker_remaining_fraction,
    ) || !server_is_in_slow_tail(
        inner,
        current_server,
        config.quote_tail_slow_worker_fraction,
    ) {
        return false;
    }

    has_idle_faster_quote_worker(inner, current_server)
}

fn quote_batch_in_tail_phase(
    inner: &Arc<ManagerInner>,
    batch_id: &str,
    remaining_fraction_threshold: f64,
) -> bool {
    let batches = inner.batches.lock().unwrap();
    let Some(batch) = batches.get(batch_id) else {
        return false;
    };
    if batch.total == 0 {
        return false;
    }
    let remaining_fraction = batch.handle.queued_count() as f64 / batch.total as f64;
    remaining_fraction <= remaining_fraction_threshold
}

fn server_is_in_slow_tail(inner: &Arc<ManagerInner>, server: &str, slow_fraction: f64) -> bool {
    let health = inner.server_health.lock().unwrap();
    let mut scored = health
        .iter()
        .filter(|(_, state)| {
            state.selected
                && state.connected
                && !state.blacklisted
                && state.cooldown_until.is_none()
        })
        .filter_map(|(server, state)| server_quote_speed_score(state).map(|score| (server, score)))
        .collect::<Vec<_>>();
    if scored.len() < 2
        || !scored
            .iter()
            .any(|(candidate, _)| candidate.as_str() == server)
    {
        return false;
    }
    scored.sort_by(|left, right| {
        left.1
            .partial_cmp(&right.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(right.0))
    });
    let slow_count = ((scored.len() as f64) * slow_fraction).ceil().max(1.0) as usize;
    let slow_count = slow_count.min(scored.len().saturating_sub(1));
    let slow_start = scored.len().saturating_sub(slow_count);
    scored
        .iter()
        .position(|(candidate, _)| candidate.as_str() == server)
        .is_some_and(|index| index >= slow_start)
}

fn has_idle_faster_quote_worker(inner: &Arc<ManagerInner>, current_server: &str) -> bool {
    let (current_score, scores) = {
        let health = inner.server_health.lock().unwrap();
        let current_score = health
            .get(current_server)
            .and_then(server_quote_speed_score);
        let scores = health
            .iter()
            .filter(|(_, state)| {
                state.selected
                    && state.connected
                    && !state.blacklisted
                    && state.cooldown_until.is_none()
            })
            .filter_map(|(server, state)| {
                server_quote_speed_score(state).map(|score| (server.clone(), score))
            })
            .collect::<HashMap<_, _>>();
        (current_score, scores)
    };
    let Some(current_score) = current_score else {
        return false;
    };

    let workers = inner.workers.lock().unwrap();
    workers.iter().any(|worker| {
        let activity = worker.activity.lock().unwrap();
        activity.connected
            && activity.active_started_at.is_none()
            && activity.server != current_server
            && scores
                .get(&activity.server)
                .is_some_and(|score| *score < current_score)
    })
}

fn server_quote_speed_score(state: &ServerHealthState) -> Option<f64> {
    state
        .avg_quote_ms_per_code
        .or(state.avg_latency_ms)
        .or_else(|| state.validation_latency_ms.map(|value| value as f64))
}

fn lane_for_priority_mut(queues: &mut TaskQueues, priority: i32) -> &mut VecDeque<QueuedTask> {
    if priority >= 30 {
        &mut queues.critical
    } else if priority >= 0 {
        &mut queues.normal
    } else {
        &mut queues.background
    }
}

fn push_task_into_lane(lane: &mut VecDeque<QueuedTask>, queued: QueuedTask, front: bool) {
    let insert_at = if front {
        lane.iter()
            .position(|existing| existing.task.priority <= queued.task.priority)
            .unwrap_or(lane.len())
    } else {
        lane.iter()
            .position(|existing| existing.task.priority < queued.task.priority)
            .unwrap_or(lane.len())
    };
    lane.insert(insert_at, queued);
}

fn requeue_task(inner: &Arc<ManagerInner>, queued: QueuedTask, front: bool) {
    requeue_task_with_reason(inner, queued, front, None);
}

fn requeue_task_with_reason(
    inner: &Arc<ManagerInner>,
    queued: QueuedTask,
    front: bool,
    reason: Option<&str>,
) {
    let QueuedTask {
        batch_id,
        mut task,
        attempts: previous_attempts,
        ..
    } = queued;
    let handle = {
        let batches = inner.batches.lock().unwrap();
        batches.get(&batch_id).map(|batch| batch.handle.clone())
    };
    if let Some(handle) = handle {
        handle.mark_requeued_for_execution();
    }
    let attempts = if reason.is_some() {
        previous_attempts.saturating_add(1)
    } else {
        previous_attempts
    };
    if let Some(reason) = reason {
        task.metadata
            .insert("quote_requeue_attempts".to_string(), attempts.to_string());
        task.metadata
            .insert("quote_requeue_reason".to_string(), reason.to_string());
        if reason == "deadline_miss" {
            task.metadata.insert(
                "deadline_requeue_attempts".to_string(),
                attempts.to_string(),
            );
        }
    }
    let queued = QueuedTask {
        batch_id,
        counts_for_queue_drain: false,
        attempts,
        task,
    };
    let mut queues = inner.queues.lock().unwrap();
    let lane = lane_for_priority_mut(&mut queues, queued.task.priority);
    push_task_into_lane(lane, queued, front);
    inner.queue_ready.notify_one();
}

fn task_result_logical_id(result: &TaskResult) -> Option<String> {
    if !result.task_id.is_empty() {
        return Some(result.task_id.clone());
    }
    if !result.stock_code.is_empty() {
        return Some(result.stock_code.clone());
    }
    if result.task_type != TaskType::Quote {
        return Some(format!("{:?}:{}", result.task_type, result.market));
    }
    None
}

fn task_logical_id(task: &Task) -> String {
    task.key()
}

fn batch_task_key(batch_id: &str, task_id: impl Into<String>) -> BatchTaskKey {
    BatchTaskKey {
        batch_id: batch_id.to_string(),
        task_id: task_id.into(),
    }
}

fn remove_active_quote_tasks_for_batch(inner: &Arc<ManagerInner>, batch_id: &str) {
    let mut active = inner.active_quote_tasks.lock().unwrap();
    active.retain(|key, _| key.batch_id != batch_id);
}

fn mark_quote_task_started(inner: &Arc<ManagerInner>, batch_id: &str, task: &Task, server: &str) {
    if task.task_type != TaskType::Quote || task.quote_stocks.is_empty() {
        return;
    }
    if task.metadata.contains_key("quote_hedge_of") {
        return;
    }
    let key = batch_task_key(batch_id, task_logical_id(task));
    let mut active = inner.active_quote_tasks.lock().unwrap();
    active.insert(
        key,
        ActiveQuoteTask {
            batch_id: batch_id.to_string(),
            task: task.clone(),
            server: server.to_string(),
            started_at: Instant::now(),
        },
    );
}

fn mark_quote_task_finished(inner: &Arc<ManagerInner>, batch_id: &str, task_id: &str) {
    let mut active = inner.active_quote_tasks.lock().unwrap();
    active.remove(&batch_task_key(batch_id, task_id.to_string()));
}

fn enqueue_due_quote_hedges(inner: &Arc<ManagerInner>, queues: &mut TaskQueues) -> usize {
    let config = current_config(inner);
    if config.quote_hedge_after_ms == 0 || config.quote_hedge_max_fraction <= 0.0 {
        return 0;
    }

    let mut due = {
        let active = inner.active_quote_tasks.lock().unwrap();
        active.values().cloned().collect::<Vec<_>>()
    };
    due.sort_by_key(|active| active.started_at);

    let mut hedges = Vec::new();
    {
        let mut batches = inner.batches.lock().unwrap();
        for active in due {
            let Some(batch) = batches.get_mut(&active.batch_id) else {
                continue;
            };
            if batch.handle.queued_count() > 0 {
                continue;
            }
            let batch_elapsed_ms = batch.handle.elapsed_ms();
            if batch_elapsed_ms < u128::from(config.quote_hedge_after_ms) {
                continue;
            }
            let task_id = task_logical_id(&active.task);
            if batch.completed_task_ids.contains(&task_id)
                || batch.hedge_enqueued_task_ids.contains(&task_id)
            {
                continue;
            }
            let max_hedges = ((batch.total as f64) * config.quote_hedge_max_fraction)
                .ceil()
                .max(1.0) as usize;
            if batch.hedge_enqueued_task_ids.len() >= max_hedges {
                continue;
            }
            batch.hedge_enqueued_task_ids.insert(task_id.clone());
            let mut task = active.task.clone();
            task.metadata
                .insert("quote_hedge_of".to_string(), task_id.clone());
            task.metadata.insert(
                "quote_hedge_after_ms".to_string(),
                config.quote_hedge_after_ms.to_string(),
            );
            task.metadata.insert(
                "quote_hedge_original_server".to_string(),
                active.server.clone(),
            );
            log::info!(
                "tdx quote hedge enqueue batch_id={} task_id={} original_server={} active_elapsed_ms={} batch_elapsed_ms={} hedge_after_ms={} max_fraction={}",
                active.batch_id,
                task_id,
                active.server,
                active.started_at.elapsed().as_millis(),
                batch_elapsed_ms,
                config.quote_hedge_after_ms,
                config.quote_hedge_max_fraction
            );
            hedges.push(QueuedTask {
                batch_id: active.batch_id,
                task,
                counts_for_queue_drain: false,
                attempts: 0,
            });
        }
    }

    let count = hedges.len();
    for queued in hedges {
        let lane = lane_for_priority_mut(queues, queued.task.priority);
        push_task_into_lane(lane, queued, true);
    }
    count
}

fn next_quote_hedge_due_in(inner: &Arc<ManagerInner>) -> Option<Duration> {
    let config = current_config(inner);
    if config.quote_hedge_after_ms == 0 || config.quote_hedge_max_fraction <= 0.0 {
        return None;
    }

    let hedge_after = Duration::from_millis(config.quote_hedge_after_ms);
    let active = inner.active_quote_tasks.lock().unwrap();
    let batches = inner.batches.lock().unwrap();
    active
        .values()
        .filter_map(|active| {
            let batch = batches.get(&active.batch_id)?;
            if batch.handle.queued_count() > 0 {
                return None;
            }
            let task_id = task_logical_id(&active.task);
            if batch.completed_task_ids.contains(&task_id)
                || batch.hedge_enqueued_task_ids.contains(&task_id)
            {
                return None;
            }
            let max_hedges = ((batch.total as f64) * config.quote_hedge_max_fraction)
                .ceil()
                .max(1.0) as usize;
            if batch.hedge_enqueued_task_ids.len() >= max_hedges {
                return None;
            }
            Some(
                hedge_after
                    .checked_sub(Duration::from_millis(
                        batch.handle.elapsed_ms().min(u128::from(u64::MAX)) as u64,
                    ))
                    .unwrap_or(Duration::ZERO),
            )
        })
        .min()
}

fn deliver_result(inner: &Arc<ManagerInner>, batch_id: &str, result: TaskResult) {
    let logical_task_id = task_result_logical_id(&result);
    let mut should_mark_finished = false;
    let uses_default_results = batch_uses_default_results(batch_id);
    let mut default_result = None;
    let mut remove_finished_batch = false;
    let mut batches = inner.batches.lock().unwrap();
    if let Some(batch) = batches.get_mut(batch_id) {
        if let Some(task_id) = logical_task_id.as_ref()
            && !batch.completed_task_ids.insert(task_id.clone())
        {
            drop(batches);
            mark_quote_task_finished(inner, batch_id, task_id);
            return;
        }
        inner.completed_tasks.fetch_add(1, Ordering::SeqCst);
        batch.completed += 1;
        should_mark_finished = true;
        if !batch.handle.is_cancelled() {
            if uses_default_results {
                default_result = Some(result.clone());
            } else {
                batch.results.push(result.clone());
            }
            let _ = batch.handle.enqueue(result, true);
        }
        if batch.completed >= batch.total {
            batch.handle.mark_finished();
            remove_finished_batch = uses_default_results;
        }
    }
    if remove_finished_batch {
        batches.remove(batch_id);
    }
    drop(batches);
    if should_mark_finished && let Some(task_id) = logical_task_id.as_ref() {
        mark_quote_task_finished(inner, batch_id, task_id);
    }

    if let Some(result) = default_result {
        inner.default_results.lock().unwrap().push_back(result);
    }
}

fn batch_uses_default_results(batch_id: &str) -> bool {
    batch_id.starts_with("default_")
}

fn seed_server_health(inner: &Arc<ManagerInner>, servers: &[String]) {
    let mut health = inner.server_health.lock().unwrap();
    health.clear();
    for server in servers {
        health.insert(server.clone(), ServerHealthState::default());
    }
}

fn select_servers_for_startup(
    inner: &Arc<ManagerInner>,
    servers: &[String],
    stock_codes: &[String],
    desired_workers: usize,
    client_mode: ClientMode,
) -> Vec<String> {
    let worker_count = desired_workers.max(1).min(servers.len().max(1));
    let selected = if client_mode == ClientMode::Real {
        select_validated_real_servers(inner, servers, stock_codes, worker_count)
    } else {
        servers
            .iter()
            .take(worker_count)
            .cloned()
            .collect::<Vec<_>>()
    };
    mark_selected_servers(inner, &selected);
    selected
}

fn select_validated_real_servers(
    inner: &Arc<ManagerInner>,
    servers: &[String],
    stock_codes: &[String],
    worker_count: usize,
) -> Vec<String> {
    let config = current_config(inner);
    let benchmark_stocks = stock_codes
        .iter()
        .take(config.startup_benchmark_quote_count)
        .map(|code| (market_for_code(code), code.clone()))
        .collect::<Vec<_>>();
    let startup_read_timeout = (config.startup_blacklist_latency_threshold_ms > 0)
        .then(|| Duration::from_millis(config.startup_blacklist_latency_threshold_ms));
    let handles = servers
        .iter()
        .cloned()
        .map(|server| {
            let benchmark_stocks = benchmark_stocks.clone();
            thread::spawn(move || {
                let api = TdxHqApi::real(server.clone());
                let mut error = String::new();
                let mut latency_ms = None;
                let success = match api.connect_to_server() {
                    Ok(()) => {
                        let heartbeat_started = Instant::now();
                        match api.heartbeat() {
                            Ok(()) if benchmark_stocks.is_empty() => {
                                latency_ms = Some(
                                    heartbeat_started
                                        .elapsed()
                                        .as_millis()
                                        .min(u128::from(u64::MAX))
                                        as u64,
                                );
                                true
                            }
                            Ok(()) => {
                                let task = Task::quote(benchmark_stocks.clone());
                                let result =
                                    api.execute_task_with_read_timeout(&task, startup_read_timeout);
                                latency_ms = Some(
                                    result.download_time.as_millis().min(u128::from(u64::MAX))
                                        as u64,
                                );
                                if result.success && result.quote_count == benchmark_stocks.len() {
                                    true
                                } else {
                                    error = if result.error_message.trim().is_empty() {
                                        format!(
                                            "startup quote benchmark returned {}/{} quotes",
                                            result.quote_count,
                                            benchmark_stocks.len()
                                        )
                                    } else {
                                        result.error_message
                                    };
                                    false
                                }
                            }
                            Err(err) => {
                                latency_ms = Some(
                                    heartbeat_started
                                        .elapsed()
                                        .as_millis()
                                        .min(u128::from(u64::MAX))
                                        as u64,
                                );
                                error = err;
                                false
                            }
                        }
                    }
                    Err(err) => {
                        error = err;
                        false
                    }
                };
                api.disconnect();
                StartupValidationResult {
                    server,
                    success,
                    latency_ms,
                    threshold_units: benchmark_stocks.len().max(1),
                    error,
                }
            })
        })
        .collect::<Vec<_>>();

    let results = handles
        .into_iter()
        .filter_map(|handle| handle.join().ok())
        .collect::<Vec<_>>();

    apply_startup_validation_results(inner, results, worker_count)
}

fn apply_startup_validation_results(
    inner: &Arc<ManagerInner>,
    mut results: Vec<StartupValidationResult>,
    worker_count: usize,
) -> Vec<String> {
    let config = current_config(inner);
    let startup_blacklist_threshold = config.startup_blacklist_latency_threshold_ms;
    let speed_filter_threshold_ms = startup_speed_filter_threshold_ms(&config, &results);
    let speed_filter_allowed = startup_speed_filter_allowed_servers(
        &config,
        &results,
        worker_count,
        speed_filter_threshold_ms,
    );
    {
        let mut health = inner.server_health.lock().unwrap();
        for result in &results {
            let threshold_ms =
                startup_blacklist_threshold.saturating_mul(result.threshold_units.max(1) as u64);
            let too_slow = result.success
                && startup_blacklist_threshold > 0
                && result
                    .latency_ms
                    .is_some_and(|latency| latency > threshold_ms);
            let too_slow_for_speed_filter = result.success
                && speed_filter_threshold_ms.is_some()
                && !speed_filter_allowed.contains(&result.server);
            let blacklisted = too_slow || too_slow_for_speed_filter;
            let state = health.entry(result.server.clone()).or_default();
            state.validated = result.success && !blacklisted;
            state.validation_latency_ms = result.latency_ms;
            state.blacklisted = blacklisted;
            state.connected = false;
            state.selected = false;
            state.last_error = if too_slow {
                format!(
                    "startup benchmark latency {}ms exceeded threshold {}ms",
                    result.latency_ms.unwrap_or(0),
                    threshold_ms
                )
            } else if too_slow_for_speed_filter {
                format!(
                    "startup benchmark latency {}ms exceeded speed filter threshold {}ms",
                    result.latency_ms.unwrap_or(0),
                    speed_filter_threshold_ms.unwrap_or(0)
                )
            } else if result.success {
                String::new()
            } else {
                result.error.clone()
            };
        }
    }

    let config = current_config(inner);
    let server_scores = {
        let health = inner.server_health.lock().unwrap();
        results
            .iter()
            .map(|result| {
                let score = health
                    .get(&result.server)
                    .map(|state| score_server_state(state, &config))
                    .unwrap_or(f64::MAX);
                (result.server.clone(), score)
            })
            .collect::<HashMap<_, _>>()
    };

    results.sort_by(|left, right| {
        right
            .success
            .cmp(&left.success)
            .then_with(|| {
                server_scores[&left.server]
                    .partial_cmp(&server_scores[&right.server])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.latency_ms
                    .unwrap_or(u64::MAX)
                    .cmp(&right.latency_ms.unwrap_or(u64::MAX))
            })
            .then_with(|| left.server.cmp(&right.server))
    });

    persist_server_health(inner, false);

    let mut selected = results
        .iter()
        .filter(|value| value.success)
        .filter(|value| {
            let health = inner.server_health.lock().unwrap();
            health
                .get(&value.server)
                .is_some_and(|state| state.validated && !state.blacklisted)
        })
        .take(worker_count)
        .map(|value| value.server.clone())
        .collect::<Vec<_>>();

    if selected.len() < worker_count {
        let health = inner.server_health.lock().unwrap();
        for result in &results {
            if selected.len() >= worker_count {
                break;
            }
            if result.success
                && health
                    .get(&result.server)
                    .is_some_and(|state| state.validated && !state.blacklisted)
                && !selected.contains(&result.server)
            {
                selected.push(result.server.clone());
            }
        }
    }

    persist_server_health(inner, false);
    selected
}

fn startup_speed_filter_threshold_ms(
    config: &ManagerConfig,
    results: &[StartupValidationResult],
) -> Option<u64> {
    if config.startup_speed_filter_ratio <= 0.0 {
        return None;
    }
    let fastest = results
        .iter()
        .filter(|result| result.success)
        .filter_map(|result| result.latency_ms)
        .min()?;
    Some(((fastest as f64) * config.startup_speed_filter_ratio).ceil() as u64)
}

fn startup_speed_filter_allowed_servers(
    config: &ManagerConfig,
    results: &[StartupValidationResult],
    worker_count: usize,
    threshold_ms: Option<u64>,
) -> HashSet<String> {
    let Some(threshold_ms) = threshold_ms else {
        return results
            .iter()
            .filter(|result| result.success)
            .map(|result| result.server.clone())
            .collect();
    };
    let mut successful = results
        .iter()
        .filter(|result| result.success)
        .filter_map(|result| {
            result
                .latency_ms
                .map(|latency| (result.server.clone(), latency))
        })
        .collect::<Vec<_>>();
    successful.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));

    let min_workers = config
        .startup_speed_filter_min_workers
        .min(worker_count)
        .min(successful.len());
    let mut allowed = successful
        .iter()
        .filter(|(_, latency)| *latency <= threshold_ms)
        .map(|(server, _)| server.clone())
        .collect::<HashSet<_>>();
    for (server, _) in successful {
        if allowed.len() >= min_workers {
            break;
        }
        allowed.insert(server);
    }
    allowed
}

fn mark_selected_servers(inner: &Arc<ManagerInner>, selected: &[String]) {
    let mut health = inner.server_health.lock().unwrap();
    for state in health.values_mut() {
        state.selected = false;
    }
    for server in selected {
        health.entry(server.clone()).or_default().selected = true;
    }
}

fn switch_selected_server(inner: &Arc<ManagerInner>, old_server: &str, new_server: &str) {
    let mut health = inner.server_health.lock().unwrap();
    if let Some(state) = health.get_mut(old_server) {
        state.selected = false;
        state.connected = false;
    }
    health.entry(new_server.to_string()).or_default().selected = true;
}

fn mark_connected(inner: &Arc<ManagerInner>, server: &str, connected: bool) {
    let mut health = inner.server_health.lock().unwrap();
    let state = health.entry(server.to_string()).or_default();
    state.connected = connected;
    if connected {
        state.consecutive_failures = 0;
        state.cooldown_until = None;
        state.last_error.clear();
    }
}

fn escalated_connection_failure_cooldown(
    config: &ManagerConfig,
    consecutive_failures: usize,
) -> Duration {
    let escalation_steps = consecutive_failures
        .saturating_sub(config.failure_cooldown_threshold)
        .min(31) as u32;
    config
        .failure_cooldown
        .saturating_mul(1u32 << escalation_steps)
        .min(config.failure_cooldown_max)
}

fn record_connection_failure(inner: &Arc<ManagerInner>, server: &str, error: &str) {
    let config = current_config(inner);
    let mut health = inner.server_health.lock().unwrap();
    let state = health.entry(server.to_string()).or_default();
    state.connected = false;
    state.failure_count += 1;
    state.consecutive_failures += 1;
    state.last_error = error.to_string();
    if state.consecutive_failures >= config.failure_cooldown_threshold {
        let cooldown = escalated_connection_failure_cooldown(&config, state.consecutive_failures);
        state.cooldown_until = Some(Instant::now() + cooldown);
        log::warn!(
            "tdx server connection failure cooldown server={} consecutive_failures={} cooldown_ms={} error={}",
            server,
            state.consecutive_failures,
            cooldown.as_millis(),
            error
        );
    } else {
        log::warn!(
            "tdx server connection failure server={} consecutive_failures={} threshold={} error={}",
            server,
            state.consecutive_failures,
            config.failure_cooldown_threshold,
            error
        );
    }
    drop(health);
    persist_server_health(inner, false);
}

fn record_task_result(inner: &Arc<ManagerInner>, server: &str, result: &TaskResult) {
    let config = current_config(inner);
    let mut health = inner.server_health.lock().unwrap();
    let state = health.entry(server.to_string()).or_default();
    let latency_ms = result.download_time.as_millis().min(u128::from(u64::MAX)) as u64;
    state.last_latency_ms = Some(latency_ms);

    if result.success {
        state.connected = true;
        state.success_count += 1;
        state.consecutive_failures = 0;
        state.last_error.clear();
        state.avg_latency_ms = Some(match state.avg_latency_ms {
            Some(current) => current * 0.7 + latency_ms as f64 * 0.3,
            None => latency_ms as f64,
        });
        if result.task_type == TaskType::Quote && result.quote_count > 0 {
            let per_code = latency_ms as f64 / result.quote_count as f64;
            state.avg_quote_ms_per_code = Some(match state.avg_quote_ms_per_code {
                Some(current) => current * 0.7 + per_code * 0.3,
                None => per_code,
            });
        }
        if latency_ms >= config.slow_latency_threshold_ms {
            state.consecutive_slow += 1;
            if state.consecutive_slow >= config.slow_cooldown_threshold {
                state.cooldown_until = Some(Instant::now() + config.slow_cooldown);
                state.last_error = format!(
                    "cooldown due to sustained latency >= {}ms",
                    config.slow_latency_threshold_ms
                );
                log::warn!(
                    "tdx server slow cooldown server={} task_type={:?} quote_count={} elapsed_ms={} threshold_ms={} consecutive_slow={} cooldown_ms={}",
                    server,
                    result.task_type,
                    result.quote_count,
                    latency_ms,
                    config.slow_latency_threshold_ms,
                    state.consecutive_slow,
                    config.slow_cooldown.as_millis()
                );
            }
        } else {
            state.consecutive_slow = 0;
        }
    } else {
        state.connected = false;
        state.failure_count += 1;
        state.consecutive_failures += 1;
        state.consecutive_slow = 0;
        state.last_error = result.error_message.clone();
        if state.consecutive_failures >= config.failure_cooldown_threshold {
            state.cooldown_until = Some(Instant::now() + config.failure_cooldown);
            log::warn!(
                "tdx server task failure cooldown server={} task_type={:?} task_id={} quote_count={} elapsed_ms={} consecutive_failures={} cooldown_ms={} error={}",
                server,
                result.task_type,
                result.task_id,
                result.quote_count,
                latency_ms,
                state.consecutive_failures,
                config.failure_cooldown.as_millis(),
                result.error_message
            );
        } else {
            log::warn!(
                "tdx server task failure server={} task_type={:?} task_id={} quote_count={} elapsed_ms={} consecutive_failures={} threshold={} error={}",
                server,
                result.task_type,
                result.task_id,
                result.quote_count,
                latency_ms,
                state.consecutive_failures,
                config.failure_cooldown_threshold,
                result.error_message
            );
        }
    }
    drop(health);
    persist_server_health(inner, false);
}

fn quote_task_deadline(inner: &Arc<ManagerInner>, server: &str, task: &Task) -> Option<Duration> {
    if task.task_type != TaskType::Quote || task.quote_stocks.is_empty() {
        return None;
    }
    let config = current_config(inner);
    let policy = QuoteSchedulingPolicy::from(&config);
    let avg_quote_ms_per_code = {
        let health = inner.server_health.lock().unwrap();
        health
            .get(server)
            .and_then(|state| state.avg_quote_ms_per_code)
    };
    policy.deadline_for_quote_chunk(task.quote_stocks.len(), avg_quote_ms_per_code)
}

fn handle_runtime_quote_deadline_miss(
    inner: &Arc<ManagerInner>,
    server: &str,
    queued: QueuedTask,
    result: &TaskResult,
    deadline: Duration,
) -> bool {
    let config = current_config(inner);
    let policy = QuoteSchedulingPolicy::from(&config);
    if queued.task.task_type != TaskType::Quote
        || !policy.deadline_missed(result.download_time, Some(deadline))
    {
        return false;
    }
    mark_quote_task_finished(inner, &queued.batch_id, &task_logical_id(&queued.task));
    cooldown_server_after_quote_failure(
        inner,
        server,
        format!(
            "quote chunk deadline exceeded elapsed_ms={} deadline_ms={} task_id={} attempts={}",
            result.download_time.as_millis(),
            deadline.as_millis(),
            queued.task.task_id,
            queued.attempts
        ),
    );
    if policy.should_requeue_after_deadline_miss(queued.attempts) {
        log::warn!(
            "tdx quote chunk deadline miss requeue server={} task_id={} quote_count={} elapsed_ms={} deadline_ms={} attempts={} max_requeues={}",
            server,
            queued.task.task_id,
            queued.task.quote_stocks.len(),
            result.download_time.as_millis(),
            deadline.as_millis(),
            queued.attempts,
            policy.slow_max_requeues
        );
        requeue_task_with_reason(inner, queued, true, Some("deadline_miss"));
    } else {
        log::warn!(
            "tdx quote chunk deadline miss exhausted server={} task_id={} quote_count={} elapsed_ms={} deadline_ms={} attempts={} max_requeues={}",
            server,
            queued.task.task_id,
            queued.task.quote_stocks.len(),
            result.download_time.as_millis(),
            deadline.as_millis(),
            queued.attempts,
            policy.slow_max_requeues
        );
        deliver_result(
            inner,
            &queued.batch_id,
            TaskResult::failure(
                &queued.task,
                format!(
                    "quote chunk exceeded deadline after {} requeues: elapsed_ms={} deadline_ms={}",
                    queued.attempts,
                    result.download_time.as_millis(),
                    deadline.as_millis()
                ),
                server.to_string(),
                result.download_time,
            ),
        );
    }
    true
}

fn handle_runtime_empty_quote_response(
    inner: &Arc<ManagerInner>,
    server: &str,
    queued: QueuedTask,
    result: &TaskResult,
) -> bool {
    if queued.task.task_type != TaskType::Quote
        || queued.task.quote_stocks.is_empty()
        || !result.success
        || result.quote_count != 0
    {
        return false;
    }

    let config = current_config(inner);
    let policy = QuoteSchedulingPolicy::from(&config);
    mark_quote_task_finished(inner, &queued.batch_id, &task_logical_id(&queued.task));
    cooldown_server_after_quote_failure(
        inner,
        server,
        format!(
            "empty quote response task_id={} requested_quote_count={} attempts={}",
            queued.task.task_id,
            queued.task.quote_stocks.len(),
            queued.attempts
        ),
    );

    if policy.should_requeue_after_deadline_miss(queued.attempts) {
        log::warn!(
            "tdx empty quote response requeue server={} task_id={} requested_quote_count={} elapsed_ms={} attempts={} max_requeues={}",
            server,
            queued.task.task_id,
            queued.task.quote_stocks.len(),
            result.download_time.as_millis(),
            queued.attempts,
            policy.slow_max_requeues
        );
        requeue_task_with_reason(inner, queued, true, Some("empty_quote_response"));
    } else {
        log::warn!(
            "tdx empty quote response exhausted server={} task_id={} requested_quote_count={} elapsed_ms={} attempts={} max_requeues={}",
            server,
            queued.task.task_id,
            queued.task.quote_stocks.len(),
            result.download_time.as_millis(),
            queued.attempts,
            policy.slow_max_requeues
        );
        deliver_result(
            inner,
            &queued.batch_id,
            TaskResult::failure(
                &queued.task,
                format!(
                    "empty quote response after {} requeues: requested_quote_count={}",
                    queued.attempts,
                    queued.task.quote_stocks.len()
                ),
                server.to_string(),
                result.download_time,
            ),
        );
    }
    true
}

fn cooldown_server_after_quote_failure(
    inner: &Arc<ManagerInner>,
    server: &str,
    reason: impl Into<String>,
) {
    let reason = reason.into();
    let config = current_config(inner);
    let mut health = inner.server_health.lock().unwrap();
    let state = health.entry(server.to_string()).or_default();
    state.connected = false;
    state.failure_count += 1;
    state.consecutive_failures += 1;
    state.consecutive_slow = 0;
    state.cooldown_until = Some(Instant::now() + config.failure_cooldown);
    state.last_error = reason;
    log::warn!(
        "tdx server cooldown after quote failure server={} cooldown_ms={} reason={}",
        server,
        config.failure_cooldown.as_millis(),
        state.last_error
    );
    drop(health);
    persist_server_health(inner, false);
}

#[cfg(test)]
fn blacklist_server(inner: &Arc<ManagerInner>, server: &str, reason: impl Into<String>) {
    let reason = reason.into();
    let mut health = inner.server_health.lock().unwrap();
    let state = health.entry(server.to_string()).or_default();
    state.blacklisted = true;
    state.selected = false;
    state.connected = false;
    state.failure_count += 1;
    state.consecutive_failures += 1;
    state.consecutive_slow = 0;
    state.last_error = reason;
    drop(health);
    persist_server_health(inner, false);
}

fn server_is_blacklisted(inner: &Arc<ManagerInner>, server: &str) -> bool {
    inner
        .server_health
        .lock()
        .unwrap()
        .get(server)
        .is_some_and(|state| state.blacklisted)
}

fn server_cooldown_remaining(inner: &Arc<ManagerInner>, server: &str) -> Option<Duration> {
    let health = inner.server_health.lock().unwrap();
    let state = health.get(server)?;
    state.cooldown_until?.checked_duration_since(Instant::now())
}

fn claim_replacement_server(inner: &Arc<ManagerInner>, current_server: &str) -> Option<String> {
    let now = Instant::now();
    let config = current_config(inner);
    let mut health = inner.server_health.lock().unwrap();
    let next_server = health
        .iter()
        .filter(|(server, state)| {
            server.as_str() != current_server
                && !state.selected
                && state.validated
                && !state.blacklisted
                && state.cooldown_until.is_none_or(|deadline| deadline <= now)
        })
        .min_by(|(left_server, left), (right_server, right)| {
            score_server_state(left, &config)
                .partial_cmp(&score_server_state(right, &config))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left_server.cmp(right_server))
        })
        .map(|(server, _)| server.clone())?;
    health.entry(next_server.clone()).or_default().selected = true;
    Some(next_server)
}

fn score_server_state(state: &ServerHealthState, config: &ManagerConfig) -> f64 {
    let latency = state
        .avg_latency_ms
        .or_else(|| state.validation_latency_ms.map(|value| value as f64))
        .unwrap_or(10_000.0);
    latency
        + state.failure_count as f64 * config.failure_penalty_ms
        + state.consecutive_failures as f64 * config.consecutive_failure_penalty_ms
        + state.consecutive_slow as f64 * config.slow_penalty_ms
}

fn current_config(inner: &Arc<ManagerInner>) -> ManagerConfig {
    inner.config.lock().unwrap().clone()
}

fn load_server_health_history(inner: &Arc<ManagerInner>) {
    let Some(path) = current_config(inner).health_history_path else {
        return;
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };

    let mut health = inner.server_health.lock().unwrap();
    for line in contents.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() < 7 {
            continue;
        }
        let Some(state) = health.get_mut(parts[0]) else {
            continue;
        };
        state.validated = parts[1] == "1";
        state.success_count = parts[2].parse::<usize>().unwrap_or(0);
        state.failure_count = parts[3].parse::<usize>().unwrap_or(0);
        state.avg_latency_ms = parse_optional_f64(parts[4]);
        state.validation_latency_ms = parse_optional_u64(parts[5]);
        state.last_latency_ms = parse_optional_u64(parts[6]);
        if parts.len() > 7 {
            state.avg_quote_ms_per_code = parse_optional_f64(parts[7]);
        }
        if parts.len() > 8 {
            state.blacklisted = parts[8] == "1";
        }
        if parts.len() > 9 {
            state.last_error = parts[9].to_string();
        }
    }
}

fn persist_server_health(inner: &Arc<ManagerInner>, force: bool) {
    let config = current_config(inner);
    let Some(path) = config.health_history_path else {
        return;
    };

    let mut last_persist = inner.last_health_persist.lock().unwrap();
    if !force && last_persist.is_some_and(|value| value.elapsed() < config.health_persist_interval)
    {
        return;
    }

    let contents = {
        let health = inner.server_health.lock().unwrap();
        let mut rows = health
            .iter()
            .map(|(server, state)| {
                format!(
                    "{server}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    if state.validated { 1 } else { 0 },
                    state.success_count,
                    state.failure_count,
                    format_optional_f64(state.avg_latency_ms),
                    format_optional_u64(state.validation_latency_ms),
                    format_optional_u64(state.last_latency_ms),
                    format_optional_f64(state.avg_quote_ms_per_code),
                    if state.blacklisted { 1 } else { 0 },
                    state.last_error.replace('\t', " "),
                )
            })
            .collect::<Vec<_>>();
        rows.sort();
        format!("# dllhqarrow-rs server health v2\n{}\n", rows.join("\n"))
    };

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = fs::create_dir_all(parent);
    }
    if fs::write(path, contents).is_ok() {
        *last_persist = Some(Instant::now());
    }
}

fn format_optional_u64(value: Option<u64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn format_optional_f64(value: Option<f64>) -> String {
    value.map(|value| format!("{value:.3}")).unwrap_or_default()
}

fn parse_optional_u64(value: &str) -> Option<u64> {
    if value.is_empty() {
        None
    } else {
        value.parse::<u64>().ok()
    }
}

fn parse_optional_f64(value: &str) -> Option<f64> {
    if value.is_empty() {
        None
    } else {
        value.parse::<f64>().ok()
    }
}

fn env_duration_ms(name: &str) -> Option<Duration> {
    env_u64(name).map(Duration::from_millis)
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.trim().parse::<u64>().ok()
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.trim().parse::<usize>().ok()
}

fn env_f64(name: &str) -> Option<f64> {
    std::env::var(name).ok()?.trim().parse::<f64>().ok()
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::{
        BatchHandle, ClientMode, KlineType, ManagerConfig, NO_REPLACEMENT_LOG_INTERVAL, QueuedTask,
        QuoteSchedulingPolicy, ServerHealthState, StartupValidationResult, Task, TaskQueues,
        TaskResult, TaskType, TdxDataManager, WorkerActivityState, WorkerPoll, WorkerThread,
        annotate_quote_request_timing, apply_startup_validation_results, blacklist_server,
        claim_replacement_server, clamp_quote_slow_max_requeues, deliver_result,
        enqueue_due_quote_hedges, escalated_connection_failure_cooldown,
        handle_runtime_empty_quote_response, handle_runtime_quote_deadline_miss,
        load_server_health_history, mark_connected, mark_quote_task_started,
        next_quote_hedge_due_in, persist_server_health, quote_task_deadline, record_task_result,
        seed_server_health, should_log_no_replacement_server, switch_selected_server,
        take_next_task, take_next_task_from_queues,
    };
    use std::collections::HashSet;
    use std::fs;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn sample_task_result() -> TaskResult {
        TaskResult::success(
            &Task::kline("000001", 1, KlineType::KlineDaily),
            crate::models::TaskData::None,
            "mock",
            Duration::from_millis(12),
        )
    }

    #[test]
    fn initialize_creates_at_most_one_worker_per_server() {
        let manager = TdxDataManager::new();
        manager.set_client_mode(ClientMode::Mock);
        let servers = vec![
            "server-a".to_string(),
            "server-a".to_string(),
            "server-b".to_string(),
        ];

        assert!(manager.initialize(&servers, &[], 3));
        let workers = manager.get_worker_activity();
        manager.shutdown();

        assert_eq!(workers.len(), 2);
        assert_eq!(
            workers
                .iter()
                .map(|worker| worker.server.as_str())
                .collect::<HashSet<_>>()
                .len(),
            workers.len()
        );
    }

    #[test]
    fn connection_failure_cooldown_escalates_and_caps() {
        let mut config = ManagerConfig::default();
        config.failure_cooldown_threshold = 2;
        config.failure_cooldown = Duration::from_secs(15);
        config.failure_cooldown_max = Duration::from_secs(120);

        assert_eq!(
            escalated_connection_failure_cooldown(&config, 2),
            Duration::from_secs(15)
        );
        assert_eq!(
            escalated_connection_failure_cooldown(&config, 4),
            Duration::from_secs(60)
        );
        assert_eq!(
            escalated_connection_failure_cooldown(&config, 20),
            Duration::from_secs(120)
        );
    }

    #[test]
    fn successful_connection_clears_failure_streak_and_cooldown() {
        let manager = TdxDataManager::new();
        {
            let mut health = manager.inner.server_health.lock().unwrap();
            let state = health.entry("recovered".to_string()).or_default();
            state.consecutive_failures = 7;
            state.cooldown_until = Some(Instant::now() + Duration::from_secs(60));
            state.last_error = "connection refused".to_string();
        }

        mark_connected(&manager.inner, "recovered", true);

        let health = manager.inner.server_health.lock().unwrap();
        let state = &health["recovered"];
        assert!(state.connected);
        assert_eq!(state.consecutive_failures, 0);
        assert!(state.cooldown_until.is_none());
        assert!(state.last_error.is_empty());
    }

    #[test]
    fn quote_request_timing_metadata_records_worker_network_boundary() {
        let task = Task::quote(vec![(0, "000001".to_string()), (1, "600000".to_string())]);
        let mut results = vec![TaskResult::success(
            &task,
            crate::models::TaskData::Quotes(Vec::new()),
            "10.0.0.1",
            Duration::from_millis(18),
        )];

        annotate_quote_request_timing(&mut results, [2], 1_789_000_000_100, 1_789_000_000_118);

        assert_eq!(
            results[0]
                .metadata
                .get("quote_request_started_unix_ms")
                .map(String::as_str),
            Some("1789000000100")
        );
        assert_eq!(
            results[0]
                .metadata
                .get("quote_response_completed_unix_ms")
                .map(String::as_str),
            Some("1789000000118")
        );
        assert_eq!(
            results[0]
                .metadata
                .get("quote_request_code_count")
                .map(String::as_str),
            Some("2")
        );
    }

    #[test]
    fn replacement_server_prefers_fast_valid_candidate() {
        let manager = TdxDataManager::new();
        let servers = vec![
            "slow".to_string(),
            "fast".to_string(),
            "cooling".to_string(),
        ];
        seed_server_health(&manager.inner, &servers);

        let mut health = manager.inner.server_health.lock().unwrap();
        health.insert(
            "slow".to_string(),
            ServerHealthState {
                selected: true,
                validated: true,
                validation_latency_ms: Some(90),
                ..ServerHealthState::default()
            },
        );
        health.insert(
            "fast".to_string(),
            ServerHealthState {
                validated: true,
                validation_latency_ms: Some(20),
                ..ServerHealthState::default()
            },
        );
        health.insert(
            "cooling".to_string(),
            ServerHealthState {
                validated: true,
                validation_latency_ms: Some(10),
                cooldown_until: Some(Instant::now() + Duration::from_secs(10)),
                ..ServerHealthState::default()
            },
        );
        drop(health);

        assert_eq!(
            claim_replacement_server(&manager.inner, "slow"),
            Some("fast".to_string())
        );

        switch_selected_server(&manager.inner, "slow", "fast");
        let health = manager.inner.server_health.lock().unwrap();
        assert!(!health["slow"].selected);
        assert!(health["fast"].selected);
    }

    #[test]
    fn replacement_server_claim_is_atomic() {
        let manager = TdxDataManager::new();
        seed_server_health(
            &manager.inner,
            &[
                "worker-a".to_string(),
                "worker-b".to_string(),
                "spare".to_string(),
            ],
        );
        {
            let mut health = manager.inner.server_health.lock().unwrap();
            for server in ["worker-a", "worker-b"] {
                let state = health.get_mut(server).unwrap();
                state.selected = true;
                state.validated = true;
                state.cooldown_until = Some(Instant::now() + Duration::from_secs(10));
            }
            let spare = health.get_mut("spare").unwrap();
            spare.validated = true;
            spare.validation_latency_ms = Some(10);
        }

        assert_eq!(
            claim_replacement_server(&manager.inner, "worker-a"),
            Some("spare".to_string())
        );
        assert_eq!(claim_replacement_server(&manager.inner, "worker-b"), None);
    }

    #[test]
    fn no_replacement_server_warning_is_rate_limited_per_server() {
        let manager = TdxDataManager::new();
        seed_server_health(&manager.inner, &["offline".to_string()]);

        assert!(should_log_no_replacement_server(&manager.inner, "offline"));
        assert!(!should_log_no_replacement_server(&manager.inner, "offline"));

        {
            let mut health = manager.inner.server_health.lock().unwrap();
            health
                .get_mut("offline")
                .unwrap()
                .last_no_replacement_log_at =
                Instant::now().checked_sub(NO_REPLACEMENT_LOG_INTERVAL + Duration::from_secs(1));
        }

        assert!(should_log_no_replacement_server(&manager.inner, "offline"));
    }

    #[test]
    fn health_history_persists_and_loads_seeded_servers() {
        let unique = format!(
            "rusthq-health-{}-{}.tsv",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);

        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.health_history_path = Some(path.clone());
        config.health_persist_interval = Duration::ZERO;
        manager.set_config(config.clone());
        seed_server_health(
            &manager.inner,
            &["persisted".to_string(), "other".to_string()],
        );

        {
            let mut health = manager.inner.server_health.lock().unwrap();
            let state = health.get_mut("persisted").unwrap();
            state.validated = true;
            state.success_count = 7;
            state.failure_count = 2;
            state.avg_latency_ms = Some(45.5);
            state.validation_latency_ms = Some(31);
            state.last_latency_ms = Some(40);
        }
        persist_server_health(&manager.inner, true);

        let restored = TdxDataManager::new();
        restored.set_config(config);
        seed_server_health(&restored.inner, &["persisted".to_string()]);
        load_server_health_history(&restored.inner);

        let health = restored.inner.server_health.lock().unwrap();
        let state = health.get("persisted").unwrap();
        assert!(state.validated);
        assert_eq!(state.success_count, 7);
        assert_eq!(state.failure_count, 2);
        assert_eq!(state.validation_latency_ms, Some(31));
        assert_eq!(state.last_latency_ms, Some(40));
        assert_eq!(state.avg_latency_ms, Some(45.5));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn worker_activity_snapshot_reports_idle_and_active_workers() {
        let manager = TdxDataManager::new();
        let idle_activity = Arc::new(Mutex::new(WorkerActivityState {
            server: "idle-server".to_string(),
            connected: true,
            ..WorkerActivityState::default()
        }));
        let active_activity = Arc::new(Mutex::new(WorkerActivityState {
            server: "busy-server".to_string(),
            connected: true,
            active_batch_id: "quote_batch".to_string(),
            active_task_id: "quote_batch_chunk_0001".to_string(),
            active_task_type: Some(TaskType::Quote),
            active_quote_count: 10,
            active_started_at: Some(Instant::now() - Duration::from_millis(25)),
        }));
        {
            let mut workers = manager.inner.workers.lock().unwrap();
            workers.push(WorkerThread {
                pending: Arc::new(AtomicUsize::new(0)),
                activity: idle_activity,
                handle: None,
            });
            workers.push(WorkerThread {
                pending: Arc::new(AtomicUsize::new(1)),
                activity: active_activity,
                handle: None,
            });
        }

        let snapshot = manager.get_worker_activity();

        assert_eq!(snapshot.len(), 2);
        let idle = snapshot
            .iter()
            .find(|worker| worker.server == "idle-server")
            .expect("idle worker");
        let busy = snapshot
            .iter()
            .find(|worker| worker.server == "busy-server")
            .expect("busy worker");
        assert!(idle.connected);
        assert!(!idle.active);
        assert!(busy.active);
        assert_eq!(busy.active_batch_id.as_deref(), Some("quote_batch"));
        assert_eq!(
            busy.active_task_id.as_deref(),
            Some("quote_batch_chunk_0001")
        );
        assert_eq!(busy.active_task_type, Some(TaskType::Quote));
        assert_eq!(busy.active_quote_count, 10);
        assert!(busy.active_elapsed_ms.unwrap() >= 20);
    }

    #[test]
    fn default_batches_flow_into_default_queue_and_cleanup_batch_records() {
        let manager = TdxDataManager::new();
        let batch_id = "default_00000001".to_string();
        manager.insert_batch_handle(
            batch_id.clone(),
            Arc::new(BatchHandle::new(batch_id.clone(), 1)),
            1,
            TaskType::Kline,
            KlineType::KlineDaily,
        );

        deliver_result(&manager.inner, &batch_id, sample_task_result());

        assert_eq!(manager.get_completed_results_limit(8).len(), 1);
        assert!(manager.get_batch_results(&batch_id).is_empty());
        assert_eq!(manager.get_batch_progress(&batch_id), (0, 0));
    }

    #[test]
    fn explicit_batches_stay_out_of_default_queue_and_can_be_taken() {
        let manager = TdxDataManager::new();
        let batch_id = "quote_00000001".to_string();
        manager.insert_batch_handle(
            batch_id.clone(),
            Arc::new(BatchHandle::new(batch_id.clone(), 1)),
            1,
            TaskType::Quote,
            KlineType::KlineDaily,
        );

        deliver_result(&manager.inner, &batch_id, sample_task_result());

        assert!(manager.get_completed_results_limit(8).is_empty());
        assert_eq!(manager.get_batch_results(&batch_id).len(), 1);
        assert_eq!(manager.take_batch_results(&batch_id).len(), 1);
        assert!(manager.get_batch_results(&batch_id).is_empty());
    }

    #[test]
    fn duplicate_quote_chunk_results_complete_logical_task_once() {
        let manager = TdxDataManager::new();
        let batch_id = "quote_00000002".to_string();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), 1));
        manager.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            1,
            TaskType::Quote,
            KlineType::KlineDaily,
        );

        let task = Task::quote(vec![(0, "000001".to_string())]).with_task_id("chunk_0");
        let first = TaskResult::success(
            &task,
            crate::models::TaskData::Quotes(Vec::new()),
            "fast",
            Duration::from_millis(20),
        );
        let duplicate = TaskResult::success(
            &task,
            crate::models::TaskData::Quotes(Vec::new()),
            "slow",
            Duration::from_millis(200),
        );

        deliver_result(&manager.inner, &batch_id, first.clone());
        deliver_result(&manager.inner, &batch_id, duplicate);

        assert_eq!(handle.finished_count(), 1);
        assert_eq!(manager.get_batch_progress(&batch_id), (1, 1));
        assert_eq!(manager.get_batch_results(&batch_id), vec![first]);
    }

    #[test]
    fn queue_scheduler_interleaves_normal_work_after_critical_burst() {
        let mut queues = TaskQueues::default();
        for index in 0..7 {
            queues.critical.push_back(QueuedTask {
                batch_id: format!("critical_{index}"),
                task: Task::kline(format!("600{index:03}"), 1, KlineType::KlineDaily),
                counts_for_queue_drain: true,
                attempts: 0,
            });
        }
        queues.normal.push_back(QueuedTask {
            batch_id: "normal_0".to_string(),
            task: Task::security_list(0),
            counts_for_queue_drain: true,
            attempts: 0,
        });

        let mut picked = Vec::new();
        for _ in 0..7 {
            let task = take_next_task_from_queues(&mut queues).expect("queued task");
            picked.push(task.batch_id);
        }

        assert_eq!(
            &picked[..6],
            &[
                "critical_0",
                "critical_1",
                "critical_2",
                "critical_3",
                "critical_4",
                "critical_5",
            ]
        );
        assert_eq!(picked[6], "normal_0");
    }

    #[test]
    fn enqueue_preserves_higher_numeric_priority_within_lane() {
        let manager = TdxDataManager::new();
        manager.enqueue_task_batch(
            "quote_priority".to_string(),
            vec![
                Task::quote(vec![(0, "000001".to_string())])
                    .with_priority(35)
                    .with_task_id("mid"),
                Task::quote(vec![(0, "000002".to_string())])
                    .with_priority(45)
                    .with_task_id("high"),
                Task::quote(vec![(0, "000003".to_string())])
                    .with_priority(40)
                    .with_task_id("higher"),
            ],
        );

        let mut queues = manager.inner.queues.lock().unwrap();
        let first = take_next_task_from_queues(&mut queues).expect("first queued task");
        let second = take_next_task_from_queues(&mut queues).expect("second queued task");
        let third = take_next_task_from_queues(&mut queues).expect("third queued task");

        assert_eq!(first.task.task_id, "high");
        assert_eq!(second.task.task_id, "higher");
        assert_eq!(third.task.task_id, "mid");
    }

    #[test]
    fn startup_validation_blacklists_servers_slower_than_threshold() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.startup_blacklist_latency_threshold_ms = 1_000;
        manager.set_config(config);
        seed_server_health(
            &manager.inner,
            &[
                "fast-startup".to_string(),
                "slow-startup".to_string(),
                "failed-startup".to_string(),
            ],
        );

        let selected = apply_startup_validation_results(
            &manager.inner,
            vec![
                StartupValidationResult {
                    server: "slow-startup".to_string(),
                    success: true,
                    latency_ms: Some(1_250),
                    threshold_units: 1,
                    error: String::new(),
                },
                StartupValidationResult {
                    server: "fast-startup".to_string(),
                    success: true,
                    latency_ms: Some(240),
                    threshold_units: 1,
                    error: String::new(),
                },
                StartupValidationResult {
                    server: "failed-startup".to_string(),
                    success: false,
                    latency_ms: Some(300),
                    threshold_units: 1,
                    error: "connect failed".to_string(),
                },
            ],
            3,
        );

        assert_eq!(selected, vec!["fast-startup"]);
        let health = manager.inner.server_health.lock().unwrap();
        assert!(health["slow-startup"].blacklisted);
        assert_eq!(
            health["slow-startup"].last_error,
            "startup benchmark latency 1250ms exceeded threshold 1000ms"
        );
        assert!(!health["fast-startup"].blacklisted);
        assert!(!health["failed-startup"].blacklisted);
    }

    #[test]
    fn startup_validation_filters_extreme_tail_but_preserves_min_workers() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.startup_blacklist_latency_threshold_ms = 0;
        config.startup_speed_filter_ratio = 4.0;
        config.startup_speed_filter_min_workers = 3;
        manager.set_config(config);
        seed_server_health(
            &manager.inner,
            &[
                "fast-a".to_string(),
                "fast-b".to_string(),
                "medium".to_string(),
                "tail".to_string(),
            ],
        );

        let selected = apply_startup_validation_results(
            &manager.inner,
            vec![
                StartupValidationResult {
                    server: "tail".to_string(),
                    success: true,
                    latency_ms: Some(400),
                    threshold_units: 1,
                    error: String::new(),
                },
                StartupValidationResult {
                    server: "fast-a".to_string(),
                    success: true,
                    latency_ms: Some(20),
                    threshold_units: 1,
                    error: String::new(),
                },
                StartupValidationResult {
                    server: "medium".to_string(),
                    success: true,
                    latency_ms: Some(60),
                    threshold_units: 1,
                    error: String::new(),
                },
                StartupValidationResult {
                    server: "fast-b".to_string(),
                    success: true,
                    latency_ms: Some(30),
                    threshold_units: 1,
                    error: String::new(),
                },
            ],
            4,
        );

        assert_eq!(selected, vec!["fast-a", "fast-b", "medium"]);
        let health = manager.inner.server_health.lock().unwrap();
        assert!(!health["medium"].blacklisted);
        assert!(health["tail"].blacklisted);
        assert_eq!(
            health["tail"].last_error,
            "startup benchmark latency 400ms exceeded speed filter threshold 80ms"
        );
    }

    #[test]
    fn quote_deadline_uses_average_per_code_times_chunk_size() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.quote_default_timeout_per_code_ms = 1;
        manager.set_config(config);
        seed_server_health(&manager.inner, &["fast".to_string()]);
        {
            let mut health = manager.inner.server_health.lock().unwrap();
            health.get_mut("fast").unwrap().avg_quote_ms_per_code = Some(125.0);
        }

        let task = Task::quote(vec![
            (0, "000001".to_string()),
            (0, "000002".to_string()),
            (0, "000003".to_string()),
        ]);

        assert_eq!(
            quote_task_deadline(&manager.inner, "fast", &task),
            Some(Duration::from_millis(375))
        );
    }

    #[test]
    fn quote_policy_uses_default_as_floor_for_average_latency() {
        let policy = QuoteSchedulingPolicy {
            default_timeout_per_code_ms: 500,
            min_timeout_ms: 250,
            slow_max_requeues: 2,
        };

        assert_eq!(
            policy.deadline_for_quote_chunk(10, Some(120.0)),
            Some(Duration::from_millis(5_000))
        );
    }

    #[test]
    fn quote_policy_applies_min_timeout() {
        let policy = QuoteSchedulingPolicy {
            default_timeout_per_code_ms: 1,
            min_timeout_ms: 250,
            slow_max_requeues: 2,
        };

        assert_eq!(
            policy.deadline_for_quote_chunk(3, Some(5.0)),
            Some(Duration::from_millis(250))
        );
    }

    #[test]
    fn quote_policy_requeues_until_attempt_limit() {
        let policy = QuoteSchedulingPolicy {
            default_timeout_per_code_ms: 500,
            min_timeout_ms: 250,
            slow_max_requeues: 2,
        };

        assert!(policy.should_requeue_after_deadline_miss(0));
        assert!(policy.should_requeue_after_deadline_miss(1));
        assert!(!policy.should_requeue_after_deadline_miss(2));
    }

    #[test]
    fn quote_slow_max_requeues_clamps_to_operational_guardrail() {
        assert_eq!(clamp_quote_slow_max_requeues(0), 1);
        assert_eq!(clamp_quote_slow_max_requeues(2), 2);
        assert_eq!(clamp_quote_slow_max_requeues(999), 3);
    }

    #[test]
    fn quote_policy_normalizes_zero_chunk_size_to_one() {
        let policy = QuoteSchedulingPolicy {
            default_timeout_per_code_ms: 500,
            min_timeout_ms: 250,
            slow_max_requeues: 2,
        };

        assert_eq!(policy.quote_chunk_size(0), 1);
        assert_eq!(policy.quote_chunk_size(10), 10);
    }

    #[test]
    fn slow_quote_deadline_miss_cools_down_and_requeues_chunk() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.quote_slow_max_requeues = 2;
        manager.set_config(config);
        seed_server_health(&manager.inner, &["slow".to_string(), "fast".to_string()]);
        {
            let mut health = manager.inner.server_health.lock().unwrap();
            health.get_mut("slow").unwrap().selected = true;
            health.get_mut("fast").unwrap().validated = true;
        }

        let batch_id = "quote_deadline".to_string();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), 1));
        manager.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            1,
            TaskType::Quote,
            KlineType::KlineDaily,
        );
        let task = Task::quote(vec![(0, "000001".to_string())]).with_task_id("chunk_0");
        let queued = QueuedTask {
            batch_id,
            task: task.clone(),
            counts_for_queue_drain: false,
            attempts: 0,
        };
        let result = TaskResult::success(
            &task,
            crate::models::TaskData::Quotes(Vec::new()),
            "slow",
            Duration::from_millis(5_100),
        );

        assert!(handle_runtime_quote_deadline_miss(
            &manager.inner,
            "slow",
            queued,
            &result,
            Duration::from_secs(5),
        ));

        let health = manager.inner.server_health.lock().unwrap();
        assert!(!health["slow"].blacklisted);
        assert!(health["slow"].selected);
        assert!(health["slow"].cooldown_until.is_some());
        assert!(health["slow"].last_error.contains("quote chunk deadline"));
        drop(health);

        let mut queues = manager.inner.queues.lock().unwrap();
        let requeued = take_next_task_from_queues(&mut queues).expect("requeued task");
        assert_eq!(requeued.task.task_id, "chunk_0");
        assert_eq!(requeued.attempts, 1);
        assert_eq!(
            requeued
                .task
                .metadata
                .get("deadline_requeue_attempts")
                .map(String::as_str),
            Some("1")
        );
        assert!(!requeued.counts_for_queue_drain);
    }

    #[test]
    fn empty_successful_quote_response_is_requeued_and_marks_server_failed() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.quote_slow_max_requeues = 2;
        manager.set_config(config);
        seed_server_health(&manager.inner, &["empty".to_string()]);
        {
            let mut health = manager.inner.server_health.lock().unwrap();
            let state = health.get_mut("empty").unwrap();
            state.connected = true;
            state.selected = true;
            state.success_count = 3;
        }

        let batch_id = "quote_empty_requeue".to_string();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), 1));
        manager.insert_batch_handle(
            batch_id.clone(),
            handle,
            1,
            TaskType::Quote,
            KlineType::KlineDaily,
        );
        let task = Task::quote(vec![(0, "000001".to_string())]).with_task_id("chunk_0");
        let queued = QueuedTask {
            batch_id,
            task: task.clone(),
            counts_for_queue_drain: false,
            attempts: 0,
        };
        let result = TaskResult::success(
            &task,
            crate::models::TaskData::Quotes(Vec::new()),
            "empty",
            Duration::from_millis(20),
        );

        assert!(handle_runtime_empty_quote_response(
            &manager.inner,
            "empty",
            queued,
            &result,
        ));

        let health = manager.inner.server_health.lock().unwrap();
        let state = &health["empty"];
        assert!(!state.connected);
        assert_eq!(state.success_count, 3);
        assert_eq!(state.failure_count, 1);
        assert_eq!(state.consecutive_failures, 1);
        assert!(state.cooldown_until.is_some());
        assert!(state.last_error.contains("empty quote response"));
        drop(health);

        let mut queues = manager.inner.queues.lock().unwrap();
        let requeued = take_next_task_from_queues(&mut queues).expect("requeued quote task");
        assert_eq!(requeued.attempts, 1);
        assert_eq!(
            requeued
                .task
                .metadata
                .get("quote_requeue_attempts")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            requeued
                .task
                .metadata
                .get("quote_requeue_reason")
                .map(String::as_str),
            Some("empty_quote_response")
        );
    }

    #[test]
    fn empty_quote_retry_can_succeed_on_another_server_and_complete_batch() {
        let manager = TdxDataManager::new();
        seed_server_health(
            &manager.inner,
            &["empty".to_string(), "healthy".to_string()],
        );
        {
            let mut health = manager.inner.server_health.lock().unwrap();
            health.get_mut("empty").unwrap().selected = true;
            health.get_mut("healthy").unwrap().validated = true;
        }

        let batch_id = "quote_empty_then_success".to_string();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), 1));
        manager.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            1,
            TaskType::Quote,
            KlineType::KlineDaily,
        );
        let task = Task::quote(vec![(0, "000001".to_string())]).with_task_id("chunk_0");
        let queued = QueuedTask {
            batch_id: batch_id.clone(),
            task: task.clone(),
            counts_for_queue_drain: false,
            attempts: 0,
        };
        let empty = TaskResult::success(
            &task,
            crate::models::TaskData::Quotes(Vec::new()),
            "empty",
            Duration::from_millis(20),
        );
        assert!(handle_runtime_empty_quote_response(
            &manager.inner,
            "empty",
            queued,
            &empty,
        ));

        let retried = {
            let mut queues = manager.inner.queues.lock().unwrap();
            take_next_task_from_queues(&mut queues).expect("retried quote task")
        };
        let success = TaskResult::success(
            &retried.task,
            crate::models::TaskData::None,
            "healthy",
            Duration::from_millis(12),
        );
        assert!(!handle_runtime_empty_quote_response(
            &manager.inner,
            "healthy",
            retried,
            &success,
        ));
        record_task_result(&manager.inner, "healthy", &success);
        deliver_result(&manager.inner, &batch_id, success.clone());

        assert!(handle.is_produced());
        assert_eq!(handle.finished_count(), 1);
        assert_eq!(manager.get_batch_results(&batch_id), vec![success]);
        let health = manager.inner.server_health.lock().unwrap();
        assert_eq!(health["healthy"].success_count, 1);
        assert!(health["healthy"].connected);
    }

    #[test]
    fn repeated_empty_quote_responses_exhaust_retry_bound_with_explicit_failure() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.quote_slow_max_requeues = 2;
        manager.set_config(config);
        seed_server_health(&manager.inner, &["empty".to_string()]);

        let batch_id = "quote_empty_exhausted".to_string();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), 1));
        manager.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            1,
            TaskType::Quote,
            KlineType::KlineDaily,
        );
        let task = Task::quote(vec![(0, "000001".to_string())]).with_task_id("chunk_0");
        let mut queued = QueuedTask {
            batch_id: batch_id.clone(),
            task: task.clone(),
            counts_for_queue_drain: false,
            attempts: 0,
        };

        for expected_attempt in 1..=2 {
            let empty = TaskResult::success(
                &queued.task,
                crate::models::TaskData::Quotes(Vec::new()),
                "empty",
                Duration::from_millis(20),
            );
            assert!(handle_runtime_empty_quote_response(
                &manager.inner,
                "empty",
                queued,
                &empty,
            ));
            queued = {
                let mut queues = manager.inner.queues.lock().unwrap();
                take_next_task_from_queues(&mut queues).expect("bounded empty response retry")
            };
            assert_eq!(queued.attempts, expected_attempt);
        }

        let empty = TaskResult::success(
            &queued.task,
            crate::models::TaskData::Quotes(Vec::new()),
            "empty",
            Duration::from_millis(20),
        );
        assert!(handle_runtime_empty_quote_response(
            &manager.inner,
            "empty",
            queued,
            &empty,
        ));

        let mut queues = manager.inner.queues.lock().unwrap();
        assert!(take_next_task_from_queues(&mut queues).is_none());
        drop(queues);
        assert!(handle.is_produced());
        let results = manager.get_batch_results(&batch_id);
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].error_message.contains("empty quote response"));
        assert!(results[0].error_message.contains("2 requeues"));
    }

    #[test]
    fn nonempty_successful_quote_response_keeps_normal_success_path() {
        let manager = TdxDataManager::new();
        seed_server_health(&manager.inner, &["healthy".to_string()]);

        let task = Task::quote(vec![(0, "000001".to_string())]).with_task_id("chunk_0");
        let queued = QueuedTask {
            batch_id: "quote_nonempty".to_string(),
            task: task.clone(),
            counts_for_queue_drain: false,
            attempts: 0,
        };
        let result = TaskResult::success(
            &task,
            crate::models::TaskData::None,
            "healthy",
            Duration::from_millis(12),
        );

        assert!(!handle_runtime_empty_quote_response(
            &manager.inner,
            "healthy",
            queued,
            &result,
        ));
        record_task_result(&manager.inner, "healthy", &result);

        let health = manager.inner.server_health.lock().unwrap();
        assert_eq!(health["healthy"].success_count, 1);
        assert_eq!(health["healthy"].failure_count, 0);
        assert!(health["healthy"].connected);
        assert!(health["healthy"].last_error.is_empty());
    }

    #[test]
    fn overdue_quote_chunk_gets_one_hedged_copy_without_changing_queue_drain() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.quote_hedge_after_ms = 1;
        config.quote_hedge_max_fraction = 0.5;
        manager.set_config(config);

        let batch_id = "quote_hedge".to_string();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), 2));
        manager.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            2,
            TaskType::Quote,
            KlineType::KlineDaily,
        );
        handle.mark_dequeued_for_execution();
        handle.mark_dequeued_for_execution();
        let task = Task::quote(vec![(0, "000001".to_string())])
            .with_priority(35)
            .with_task_id("chunk_0");

        mark_quote_task_started(&manager.inner, &batch_id, &task, "slow");
        std::thread::sleep(Duration::from_millis(2));

        let mut queues = TaskQueues::default();
        assert_eq!(enqueue_due_quote_hedges(&manager.inner, &mut queues), 1);
        assert_eq!(enqueue_due_quote_hedges(&manager.inner, &mut queues), 0);

        let hedge = take_next_task_from_queues(&mut queues).expect("hedged task");
        assert_eq!(hedge.batch_id, batch_id);
        assert_eq!(hedge.task.task_id, "chunk_0");
        assert_eq!(
            hedge
                .task
                .metadata
                .get("quote_hedge_of")
                .map(String::as_str),
            Some("chunk_0")
        );
        assert_eq!(
            hedge
                .task
                .metadata
                .get("quote_hedge_original_server")
                .map(String::as_str),
            Some("slow")
        );
        assert!(!hedge.counts_for_queue_drain);
        assert_eq!(handle.queued_count(), 0);
    }

    #[test]
    fn next_quote_hedge_due_in_wakes_idle_workers_before_heartbeat() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.quote_hedge_after_ms = 900;
        config.quote_hedge_max_fraction = 0.5;
        manager.set_config(config);

        let batch_id = "quote_hedge_due".to_string();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), 1));
        manager.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            1,
            TaskType::Quote,
            KlineType::KlineDaily,
        );
        handle.mark_dequeued_for_execution();
        let task = Task::quote(vec![(0, "000001".to_string())]).with_task_id("chunk_0");
        mark_quote_task_started(&manager.inner, &batch_id, &task, "slow");

        let wait = next_quote_hedge_due_in(&manager.inner).expect("hedge wait");

        assert!(wait <= Duration::from_millis(900));
        assert!(wait < Duration::from_secs(10));
    }

    #[test]
    fn queue_drained_quote_batch_hedges_by_batch_age_not_active_age() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.quote_hedge_after_ms = 50;
        config.quote_hedge_max_fraction = 0.5;
        manager.set_config(config);

        let batch_id = "quote_queue_drained_hedge".to_string();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), 1));
        manager.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            1,
            TaskType::Quote,
            KlineType::KlineDaily,
        );
        handle.mark_dequeued_for_execution();
        std::thread::sleep(Duration::from_millis(60));
        let task = Task::quote(vec![(0, "000001".to_string())]).with_task_id("chunk_0");
        mark_quote_task_started(&manager.inner, &batch_id, &task, "slow");

        let mut queues = TaskQueues::default();
        assert_eq!(
            next_quote_hedge_due_in(&manager.inner),
            Some(Duration::ZERO)
        );
        assert_eq!(enqueue_due_quote_hedges(&manager.inner, &mut queues), 1);
    }

    #[test]
    fn tail_slow_worker_defers_quote_task_when_fast_idle_worker_can_take_it() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.quote_tail_slow_worker_remaining_fraction = 0.34;
        config.quote_tail_slow_worker_fraction = 0.34;
        config.quote_tail_slow_worker_defer_wait_ms = 1;
        manager.set_config(config);
        seed_server_health(
            &manager.inner,
            &["fast".to_string(), "medium".to_string(), "slow".to_string()],
        );
        {
            let mut health = manager.inner.server_health.lock().unwrap();
            for server in ["fast", "medium", "slow"] {
                let state = health.get_mut(server).unwrap();
                state.selected = true;
                state.connected = true;
                state.validated = true;
            }
            health.get_mut("fast").unwrap().avg_quote_ms_per_code = Some(10.0);
            health.get_mut("medium").unwrap().avg_quote_ms_per_code = Some(20.0);
            health.get_mut("slow").unwrap().avg_quote_ms_per_code = Some(300.0);
        }
        {
            let mut workers = manager.inner.workers.lock().unwrap();
            workers.push(WorkerThread {
                pending: Arc::new(AtomicUsize::new(0)),
                activity: Arc::new(Mutex::new(WorkerActivityState {
                    server: "fast".to_string(),
                    connected: true,
                    ..WorkerActivityState::default()
                })),
                handle: None,
            });
        }

        let batch_id = "quote_tail_defer".to_string();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), 3));
        manager.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            3,
            TaskType::Quote,
            KlineType::KlineDaily,
        );
        handle.mark_dequeued_for_execution();
        handle.mark_dequeued_for_execution();
        let task = Task::quote(vec![(0, "000001".to_string())])
            .with_priority(10)
            .with_task_id("chunk_2");
        manager.enqueue_task_batch(batch_id, vec![task]);

        let polled = take_next_task(&manager.inner, "slow", Some(Duration::from_millis(2)));

        assert!(matches!(polled, WorkerPoll::Timeout));
        assert_eq!(handle.queued_count(), 1);
    }

    #[test]
    fn tail_slow_worker_takes_quote_task_when_no_fast_idle_worker_exists() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.quote_tail_slow_worker_remaining_fraction = 0.34;
        config.quote_tail_slow_worker_fraction = 0.34;
        manager.set_config(config);
        seed_server_health(&manager.inner, &["fast".to_string(), "slow".to_string()]);
        {
            let mut health = manager.inner.server_health.lock().unwrap();
            for server in ["fast", "slow"] {
                let state = health.get_mut(server).unwrap();
                state.selected = true;
                state.connected = true;
                state.validated = true;
            }
            health.get_mut("fast").unwrap().avg_quote_ms_per_code = Some(10.0);
            health.get_mut("slow").unwrap().avg_quote_ms_per_code = Some(300.0);
        }
        {
            let mut workers = manager.inner.workers.lock().unwrap();
            workers.push(WorkerThread {
                pending: Arc::new(AtomicUsize::new(1)),
                activity: Arc::new(Mutex::new(WorkerActivityState {
                    server: "fast".to_string(),
                    connected: true,
                    active_started_at: Some(Instant::now()),
                    ..WorkerActivityState::default()
                })),
                handle: None,
            });
        }

        let batch_id = "quote_tail_no_fast_idle".to_string();
        let handle = Arc::new(BatchHandle::new(batch_id.clone(), 3));
        manager.insert_batch_handle(
            batch_id.clone(),
            handle.clone(),
            3,
            TaskType::Quote,
            KlineType::KlineDaily,
        );
        handle.mark_dequeued_for_execution();
        handle.mark_dequeued_for_execution();
        let task = Task::quote(vec![(0, "000001".to_string())])
            .with_priority(10)
            .with_task_id("chunk_2");
        manager.enqueue_task_batch(batch_id.clone(), vec![task]);

        let polled = take_next_task(&manager.inner, "slow", Some(Duration::from_millis(2)));

        let WorkerPoll::Task(queued) = polled else {
            panic!("expected slow worker to take task when no faster idle worker exists");
        };
        assert_eq!(queued.batch_id, batch_id);
        assert_eq!(handle.queued_count(), 0);
    }

    #[test]
    fn blacklisted_worker_without_replacement_does_not_dequeue_task() {
        let manager = TdxDataManager::new();
        let mut config = ManagerConfig::default();
        config.connect_retry_delay = Duration::from_millis(20);
        manager.set_config(config);
        manager.initialize(&["only".to_string()], &["000001".to_string()], 1);
        blacklist_server(&manager.inner, "only", "test blacklist");

        let handle = manager.start_quote_batch(&[(0, "000001".to_string())], 1);

        assert!(!handle.wait_produced(Some(Duration::from_millis(80))));
        assert_eq!(handle.queued_count(), 1);
        assert_eq!(handle.finished_count(), 0);

        manager.shutdown();
    }
}
