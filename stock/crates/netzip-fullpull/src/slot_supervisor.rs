//! Dynamic slot supervisor for official 5188 data connections.
//!
//! The supervisor owns retry/backoff/lifecycle policy only. The per-slot job
//! must perform a *complete* lifecycle on every invocation (connect, full
//! interleaved initialization, receive loop) so a retry never reuses stale
//! decoder state. Slot count comes from the caller's current assignment; the
//! captured ten-connection sample is not a constant.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Cooperative cancellation shared with slot jobs. Jobs must poll between
/// blocking steps; `wait` bounds any supervisor-side sleep.
#[derive(Clone, Default)]
pub struct StopFlag(Arc<AtomicBool>);

impl StopFlag {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Sleeps in small steps, returning early once stopped. Returns whether
    /// the full duration elapsed without a stop request.
    pub fn wait(&self, duration: Duration) -> bool {
        let deadline = Instant::now() + duration;
        loop {
            if self.is_stopped() {
                return false;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10).min(remaining));
        }
    }
}

/// Slot job failure classification. Only retryable failures reconnect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SlotError {
    Retryable(String),
    Fatal(String),
}

/// Lifecycle events surfaced to the owner. They never carry payloads or
/// credentials, only slot identity and policy numbers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotEvent {
    Attempt { slot: usize, attempt: u32 },
    Exhausted { slot: usize, attempts: u32 },
    Completed { slot: usize },
    Interrupted { slot: usize },
}

/// Bounded exponential backoff parameters.
#[derive(Clone, Copy, Debug)]
pub struct BackoffPolicy {
    pub base: Duration,
    pub max: Duration,
}

impl BackoffPolicy {
    #[must_use]
    pub fn delay(&self, attempt: u32) -> Duration {
        let shift = attempt.min(16);
        self.max.min(self.base.saturating_mul(1_u32 << shift))
    }
}

/// Handle to a running supervisor. `stop` is idempotent and joins all slots.
pub struct SlotSupervisor {
    stop: StopFlag,
    handles: Mutex<Vec<JoinHandle<()>>>,
    events: Receiver<SlotEvent>,
    slot_count: usize,
}

/// One slot job: a complete connect+initialize+receive lifecycle for the
/// given slot index. `Ok` means the slot ended intentionally (clean close);
/// it will not be reconnected.
pub type SlotJob = Arc<dyn Fn(usize, &StopFlag) -> Result<(), SlotError> + Send + Sync>;

impl SlotSupervisor {
    /// Starts one thread per slot index in `slots`. The slot list comes from
    /// the caller's current assignment and may be empty.
    pub fn start(
        slots: &[usize],
        max_attempts: u32,
        policy: BackoffPolicy,
        stop: StopFlag,
        job: SlotJob,
    ) -> Self {
        Self::try_start(slots, max_attempts, policy, stop, job)
            .expect("valid 5188 slot supervisor configuration")
    }

    /// Validates and starts one thread per unique slot index.
    pub fn try_start(
        slots: &[usize],
        max_attempts: u32,
        policy: BackoffPolicy,
        stop: StopFlag,
        job: SlotJob,
    ) -> Result<Self, String> {
        if max_attempts == 0 {
            return Err("5188 slot supervisor requires at least one attempt".to_string());
        }
        let unique = slots.iter().copied().collect::<HashSet<_>>();
        if unique.len() != slots.len() {
            return Err("5188 slot supervisor requires unique slot indexes".to_string());
        }
        let (sender, receiver) = channel();
        let mut handles = Vec::with_capacity(slots.len());
        for &slot in slots {
            let sender = sender.clone();
            let stop = stop.clone();
            let job = Arc::clone(&job);
            handles.push(
                std::thread::Builder::new()
                    .name(format!("5188-slot-{slot}"))
                    .spawn(move || {
                        let mut attempt = 0_u32;
                        loop {
                            if stop.is_stopped() {
                                let _ = sender.send(SlotEvent::Interrupted { slot });
                                return;
                            }
                            attempt += 1;
                            let _ = sender.send(SlotEvent::Attempt { slot, attempt });
                            match job(slot, &stop) {
                                Ok(()) => {
                                    let _ = sender.send(SlotEvent::Completed { slot });
                                    return;
                                }
                                Err(SlotError::Fatal(reason)) => {
                                    let _ = sender.send(SlotEvent::Exhausted {
                                        slot,
                                        attempts: attempt,
                                    });
                                    let _ = reason;
                                    return;
                                }
                                Err(SlotError::Retryable(_)) => {
                                    if attempt >= max_attempts {
                                        let _ = sender.send(SlotEvent::Exhausted {
                                            slot,
                                            attempts: attempt,
                                        });
                                        return;
                                    }
                                }
                            }
                            let delay = policy.delay(attempt.saturating_sub(1));
                            if !stop.wait(delay) {
                                let _ = sender.send(SlotEvent::Interrupted { slot });
                                return;
                            }
                        }
                    })
                    .expect("spawn 5188 slot thread"),
            );
        }
        Ok(Self {
            stop,
            handles: Mutex::new(handles),
            events: receiver,
            slot_count: slots.len(),
        })
    }

    #[must_use]
    pub fn stop_flag(&self) -> StopFlag {
        self.stop.clone()
    }

    /// Non-blocking event poll.
    #[must_use]
    pub fn poll_event(&self) -> Option<SlotEvent> {
        self.events.try_recv().ok()
    }

    /// Idempotent stop: signals all slots and joins their threads.
    pub fn stop(&self) {
        self.stop.stop();
        if let Ok(mut handles) = self.handles.lock() {
            for handle in handles.drain(..) {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for SlotSupervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Convenience: waits until every slot reaches a terminal event, returning
/// the terminal event per slot. Used by tests and callers that run the
/// supervisor to completion.
#[must_use]
pub fn join_terminal(supervisor: &SlotSupervisor, timeout: Duration) -> HashMap<usize, SlotEvent> {
    let deadline = Instant::now() + timeout;
    let mut terminal = HashMap::new();
    while terminal.len() < supervisor.slot_count && Instant::now() < deadline {
        match supervisor.poll_event() {
            Some(
                event @ (SlotEvent::Exhausted { slot, .. }
                | SlotEvent::Completed { slot }
                | SlotEvent::Interrupted { slot }),
            ) => {
                terminal.insert(slot, event);
            }
            Some(_) => {}
            None => {
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    terminal
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn backoff_is_bounded_and_exponential() {
        let policy = BackoffPolicy {
            base: Duration::from_millis(10),
            max: Duration::from_millis(40),
        };
        assert_eq!(policy.delay(0), Duration::from_millis(10));
        assert_eq!(policy.delay(1), Duration::from_millis(20));
        assert_eq!(policy.delay(2), Duration::from_millis(40));
        assert_eq!(policy.delay(9), Duration::from_millis(40));
    }

    #[test]
    fn stop_wakes_a_backoff_sleep_quickly() {
        let stop = StopFlag::new();
        let supervisor_slot = stop.clone();
        let handle = std::thread::spawn(move || {
            // Would sleep 5 seconds; must return almost immediately.
            assert!(!supervisor_slot.wait(Duration::from_secs(5)));
        });
        stop.stop();
        handle.join().expect("join sleeper");
    }

    #[test]
    fn retryable_slots_retry_independently_until_success() {
        let stop = StopFlag::new();
        let failures = [2_usize, 1]; // per-slot failure budget before success
        let counters: Vec<AtomicUsize> = (0..2).map(|_| AtomicUsize::new(0)).collect();
        let counters = Arc::new(counters);
        let failures = Arc::new(failures);
        let job: SlotJob = {
            let counters = Arc::clone(&counters);
            let failures = Arc::clone(&failures);
            Arc::new(move |slot, stop| {
                let index = counters[slot].fetch_add(1, Ordering::SeqCst);
                if index < failures[slot] {
                    return Err(SlotError::Retryable("transient".to_string()));
                }
                // Successful slots end intentionally: no reconnect.
                let _ = stop;
                Ok(())
            })
        };
        let supervisor = SlotSupervisor::start(
            &[0, 1],
            5,
            BackoffPolicy {
                base: Duration::from_millis(1),
                max: Duration::from_millis(2),
            },
            stop,
            job,
        );
        let terminal = join_terminal(&supervisor, Duration::from_secs(5));
        supervisor.stop();
        assert_eq!(terminal.len(), 2, "both slots reach a terminal event");
        for slot in 0..2 {
            assert_eq!(
                terminal.get(&slot),
                Some(&SlotEvent::Completed { slot }),
                "slot {slot} completes after its own retries"
            );
            assert!(counters[slot].load(Ordering::SeqCst) > failures[slot]);
        }
    }

    #[test]
    fn fatal_errors_stop_a_slot_without_retry() {
        let stop = StopFlag::new();
        let job: SlotJob = Arc::new(|_slot, _stop| Err(SlotError::Fatal("rejected".to_string())));
        let supervisor = SlotSupervisor::start(
            &[7],
            5,
            BackoffPolicy {
                base: Duration::from_millis(1),
                max: Duration::from_millis(1),
            },
            stop,
            job,
        );
        let terminal = join_terminal(&supervisor, Duration::from_secs(5));
        supervisor.stop();
        assert_eq!(
            terminal.get(&7),
            Some(&SlotEvent::Exhausted {
                slot: 7,
                attempts: 1
            })
        );
    }

    #[test]
    fn stop_is_idempotent_and_prevents_further_attempts() {
        let stop = StopFlag::new();
        let attempts = Arc::new(AtomicUsize::new(0));
        let job_attempts = Arc::clone(&attempts);
        let job: SlotJob = Arc::new(move |_slot, stop| {
            job_attempts.fetch_add(1, Ordering::SeqCst);
            if stop.wait(Duration::from_millis(500)) {
                // Never stopped during the job: report retryable.
                Err(SlotError::Retryable("closed".to_string()))
            } else {
                Err(SlotError::Retryable("stopped".to_string()))
            }
        });
        let supervisor = SlotSupervisor::start(
            &[3],
            50,
            BackoffPolicy {
                base: Duration::from_millis(50),
                max: Duration::from_millis(50),
            },
            stop.clone(),
            job,
        );
        std::thread::sleep(Duration::from_millis(50));
        supervisor.stop();
        supervisor.stop();
        let after_stop = attempts.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            after_stop,
            "no attempts after stop"
        );
    }

    #[test]
    fn empty_assignment_starts_and_stops_cleanly() {
        let stop = StopFlag::new();
        let job: SlotJob = Arc::new(|_slot, _stop| Ok(()));
        let supervisor = SlotSupervisor::start(
            &[],
            1,
            BackoffPolicy {
                base: Duration::from_millis(1),
                max: Duration::from_millis(1),
            },
            stop,
            job,
        );
        supervisor.stop();
        assert!(supervisor.poll_event().is_none());
    }

    #[test]
    fn join_terminal_returns_immediately_for_empty_assignment() {
        let supervisor = SlotSupervisor::start(
            &[],
            1,
            BackoffPolicy {
                base: Duration::from_secs(1),
                max: Duration::from_secs(1),
            },
            StopFlag::new(),
            Arc::new(|_slot, _stop| Ok(())),
        );
        let started = Instant::now();
        let terminal = join_terminal(&supervisor, Duration::from_secs(5));
        supervisor.stop();
        assert!(terminal.is_empty());
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn rejects_duplicate_slots_and_zero_attempts() {
        let policy = BackoffPolicy {
            base: Duration::from_millis(1),
            max: Duration::from_millis(1),
        };
        let job: SlotJob = Arc::new(|_slot, _stop| Ok(()));
        let duplicate =
            SlotSupervisor::try_start(&[1, 1], 1, policy, StopFlag::new(), Arc::clone(&job))
                .err()
                .expect("duplicate slot indexes must be rejected");
        assert!(duplicate.contains("unique"));
        let zero = SlotSupervisor::try_start(&[1], 0, policy, StopFlag::new(), job)
            .err()
            .expect("zero attempts must be rejected");
        assert!(zero.contains("at least one"));
    }
}
