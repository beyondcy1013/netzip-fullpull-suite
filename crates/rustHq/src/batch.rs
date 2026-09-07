use crate::models::TaskResult;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct BatchHandle {
    batch_id: String,
    total: AtomicUsize,
    inner: Mutex<Inner>,
    result_available: Condvar,
    space_available: Condvar,
    started_at: Instant,
}

#[derive(Debug, Default)]
struct Inner {
    queue: VecDeque<TaskResult>,
    finished: usize,
    taken: usize,
    queued: usize,
    all_produced: bool,
    cancelled: bool,
}

impl BatchHandle {
    pub const MAX_QUEUE_SIZE: usize = 200;

    pub fn new(batch_id: impl Into<String>, total_tasks: usize) -> Self {
        Self {
            batch_id: batch_id.into(),
            total: AtomicUsize::new(total_tasks),
            inner: Mutex::new(Inner {
                queued: total_tasks,
                ..Inner::default()
            }),
            result_available: Condvar::new(),
            space_available: Condvar::new(),
            started_at: Instant::now(),
        }
    }

    pub fn take_next(&self, timeout: Option<Duration>) -> Option<TaskResult> {
        let deadline = timeout.map(|value| Instant::now() + value);
        let mut inner = self.inner.lock().unwrap();

        loop {
            if let Some(result) = inner.queue.pop_front() {
                inner.taken += 1;
                self.space_available.notify_one();
                return Some(result);
            }

            if inner.cancelled || inner.all_produced {
                return None;
            }

            match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return None;
                    }
                    let (guard, wait_result) = self
                        .result_available
                        .wait_timeout(inner, deadline - now)
                        .unwrap();
                    inner = guard;
                    if wait_result.timed_out() {
                        return None;
                    }
                }
                None => {
                    inner = self.result_available.wait(inner).unwrap();
                }
            }
        }
    }

    pub fn try_take(&self) -> Option<TaskResult> {
        let mut inner = self.inner.lock().unwrap();
        let result = inner.queue.pop_front();
        if result.is_some() {
            inner.taken += 1;
            self.space_available.notify_one();
        }
        result
    }

    pub fn wait_produced(&self, timeout: Option<Duration>) -> bool {
        let deadline = timeout.map(|value| Instant::now() + value);
        let mut inner = self.inner.lock().unwrap();
        while !inner.cancelled && !inner.all_produced {
            match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return false;
                    }
                    let (guard, wait_result) = self
                        .result_available
                        .wait_timeout(inner, deadline - now)
                        .unwrap();
                    inner = guard;
                    if wait_result.timed_out() {
                        return false;
                    }
                }
                None => {
                    inner = self.result_available.wait(inner).unwrap();
                }
            }
        }
        !inner.cancelled
    }

    pub fn wait_queue_drained(&self, timeout: Option<Duration>) -> bool {
        let deadline = timeout.map(|value| Instant::now() + value);
        let mut inner = self.inner.lock().unwrap();
        while !inner.cancelled && inner.queued > 0 {
            match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return false;
                    }
                    let (guard, wait_result) = self
                        .result_available
                        .wait_timeout(inner, deadline - now)
                        .unwrap();
                    inner = guard;
                    if wait_result.timed_out() {
                        return false;
                    }
                }
                None => {
                    inner = self.result_available.wait(inner).unwrap();
                }
            }
        }
        !inner.cancelled
    }

    pub fn is_finished(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.all_produced && inner.queue.is_empty()
    }

    pub fn is_produced(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.all_produced
    }

    pub fn is_cancelled(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.cancelled
    }

    pub fn pending_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.queue.len()
    }

    pub fn queued_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.queued
    }

    pub fn finished_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.finished
    }

    pub fn total_count(&self) -> usize {
        self.total.load(Ordering::SeqCst)
    }

    pub fn taken_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.taken
    }

    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cancelled = true;
        self.result_available.notify_all();
        self.space_available.notify_all();
    }

    pub fn batch_id(&self) -> &str {
        &self.batch_id
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.started_at.elapsed().as_millis()
    }

    pub fn enqueue(&self, result: TaskResult, block_if_full: bool) -> bool {
        let mut inner = self.inner.lock().unwrap();
        while !inner.cancelled && block_if_full && inner.queue.len() >= Self::MAX_QUEUE_SIZE {
            inner = self.space_available.wait(inner).unwrap();
        }

        if inner.cancelled || (!block_if_full && inner.queue.len() >= Self::MAX_QUEUE_SIZE) {
            return false;
        }

        inner.finished += 1;
        inner.queue.push_back(result);
        self.result_available.notify_all();
        true
    }

    pub fn mark_dequeued_for_execution(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.queued = inner.queued.saturating_sub(1);
        self.result_available.notify_all();
    }

    pub fn mark_requeued_for_execution(&self) {
        let inner = self.inner.lock().unwrap();
        self.result_available.notify_all();
        drop(inner);
    }

    pub fn mark_finished(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.all_produced = true;
        self.result_available.notify_all();
    }

    pub fn set_total_tasks(&self, total: usize) {
        self.total.store(total, Ordering::SeqCst);
        let mut inner = self.inner.lock().unwrap();
        inner.queued = total;
        self.result_available.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::BatchHandle;
    use crate::models::{KlineType, Task, TaskData, TaskResult};
    use std::time::Duration;

    #[test]
    fn handle_round_trips_result_and_marks_completion() {
        let handle = BatchHandle::new("demo", 1);
        let task = Task::kline("000001", 2, KlineType::Kline1Min);
        let result = TaskResult::success(&task, TaskData::None, "mock", Duration::from_millis(1));
        assert!(handle.enqueue(result.clone(), false));
        assert_eq!(handle.try_take(), Some(result));
        handle.mark_finished();
        assert!(handle.wait_produced(Some(Duration::from_millis(1))));
        assert!(handle.is_finished());
    }

    #[test]
    fn handle_tracks_queue_drained_before_results_are_produced() {
        let handle = BatchHandle::new("demo", 2);
        assert_eq!(handle.queued_count(), 2);
        assert!(!handle.wait_queue_drained(Some(Duration::from_millis(1))));

        handle.mark_dequeued_for_execution();
        assert_eq!(handle.queued_count(), 1);
        handle.mark_requeued_for_execution();
        assert_eq!(handle.queued_count(), 1);
        handle.mark_dequeued_for_execution();

        assert_eq!(handle.queued_count(), 0);
        assert!(handle.wait_queue_drained(Some(Duration::from_millis(1))));
        assert!(!handle.is_produced());
    }
}
