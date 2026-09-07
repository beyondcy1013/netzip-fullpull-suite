use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TdxPushEvent<T> {
    pub symbol: String,
    pub source_time: u32,
    pub value: T,
}

#[derive(Clone, Debug)]
pub struct TdxPushCoalescer<T> {
    window: Duration,
    max_batch_size: usize,
    opened_at: Option<Duration>,
    pending: BTreeMap<String, TdxPushEvent<T>>,
}

impl<T> TdxPushCoalescer<T> {
    pub fn new(window: Duration, max_batch_size: usize) -> Result<Self, &'static str> {
        if window.is_zero() {
            return Err("coalescing window must be positive");
        }
        if max_batch_size == 0 {
            return Err("maximum batch size must be positive");
        }
        Ok(Self {
            window,
            max_batch_size,
            opened_at: None,
            pending: BTreeMap::new(),
        })
    }

    pub fn push(&mut self, now: Duration, event: TdxPushEvent<T>) {
        self.opened_at.get_or_insert(now);
        match self.pending.get(&event.symbol) {
            Some(current) if current.source_time > event.source_time => {}
            _ => {
                self.pending.insert(event.symbol.clone(), event);
            }
        }
    }

    pub fn drain_ready(&mut self, now: Duration) -> Vec<Vec<TdxPushEvent<T>>> {
        let Some(opened_at) = self.opened_at else {
            return Vec::new();
        };
        if now.saturating_sub(opened_at) < self.window {
            return Vec::new();
        }
        self.drain()
    }

    pub fn drain(&mut self) -> Vec<Vec<TdxPushEvent<T>>> {
        if self.pending.is_empty() {
            self.opened_at = None;
            return Vec::new();
        }
        let mut events = std::mem::take(&mut self.pending)
            .into_values()
            .collect::<Vec<_>>();
        self.opened_at = None;
        let mut batches = Vec::with_capacity(events.len().div_ceil(self.max_batch_size));
        while !events.is_empty() {
            let count = events.len().min(self.max_batch_size);
            batches.push(events.drain(..count).collect());
        }
        batches
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{TdxPushCoalescer, TdxPushEvent};
    use std::time::Duration;

    fn event(symbol: &str, source_time: u32, value: u32) -> TdxPushEvent<u32> {
        TdxPushEvent {
            symbol: symbol.to_string(),
            source_time,
            value,
        }
    }

    #[test]
    fn waits_for_window_and_keeps_latest_event_per_symbol() {
        let mut coalescer =
            TdxPushCoalescer::new(Duration::from_millis(100), 100).expect("coalescer");
        coalescer.push(Duration::from_millis(10), event("SZ000001", 110_001, 1));
        coalescer.push(Duration::from_millis(40), event("SZ000001", 110_002, 2));
        coalescer.push(Duration::from_millis(50), event("SZ000001", 110_000, 0));

        assert!(coalescer.drain_ready(Duration::from_millis(109)).is_empty());
        let batches = coalescer.drain_ready(Duration::from_millis(110));
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0], vec![event("SZ000001", 110_002, 2)]);
        assert_eq!(coalescer.pending_len(), 0);
    }

    #[test]
    fn splits_a_ready_window_into_bounded_batches() {
        let mut coalescer =
            TdxPushCoalescer::new(Duration::from_millis(100), 2).expect("coalescer");
        for index in 0..5 {
            coalescer.push(
                Duration::ZERO,
                event(&format!("SZ{index:06}"), 110_000, index),
            );
        }

        let batches = coalescer.drain_ready(Duration::from_millis(100));
        assert_eq!(batches.iter().map(Vec::len).collect::<Vec<_>>(), [2, 2, 1]);
        assert_eq!(batches.into_iter().flatten().count(), 5);
    }

    #[test]
    fn rejects_zero_sized_limits() {
        assert!(TdxPushCoalescer::<()>::new(Duration::ZERO, 1).is_err());
        assert!(TdxPushCoalescer::<()>::new(Duration::from_millis(1), 0).is_err());
    }
}
