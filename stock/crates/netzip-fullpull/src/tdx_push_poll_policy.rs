use std::collections::BTreeMap;

const RECOVERY_POLL_INTERVAL_MS: u64 = 1_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PollAction {
    FullAudit,
    RecoverShard { shard: usize },
}

#[derive(Clone, Debug)]
pub struct PushPollPolicy {
    full_audit_interval_ms: u64,
    next_full_audit_ms: u64,
    failed_shards: BTreeMap<usize, u64>,
}

impl PushPollPolicy {
    pub fn new(full_audit_interval_ms: u64) -> Self {
        let interval = full_audit_interval_ms.max(1);
        Self {
            full_audit_interval_ms: interval,
            next_full_audit_ms: interval,
            failed_shards: BTreeMap::new(),
        }
    }

    pub fn mark_shard_failed(&mut self, shard: usize, now_ms: u64) {
        self.failed_shards
            .entry(shard)
            .and_modify(|due_ms| *due_ms = (*due_ms).min(now_ms))
            .or_insert(now_ms);
    }

    pub fn mark_shard_healthy(&mut self, shard: usize) {
        self.failed_shards.remove(&shard);
    }

    pub fn take_due(&mut self, now_ms: u64) -> Vec<PollAction> {
        self.take_due_bounded(now_ms, usize::MAX)
    }

    pub fn take_due_bounded(&mut self, now_ms: u64, limit: usize) -> Vec<PollAction> {
        if limit == 0 {
            return Vec::new();
        }

        if now_ms >= self.next_full_audit_ms {
            while self.next_full_audit_ms <= now_ms {
                self.next_full_audit_ms = self
                    .next_full_audit_ms
                    .saturating_add(self.full_audit_interval_ms);
                if self.next_full_audit_ms == u64::MAX {
                    break;
                }
            }
            for due_ms in self.failed_shards.values_mut() {
                *due_ms = now_ms.saturating_add(RECOVERY_POLL_INTERVAL_MS);
            }
            return vec![PollAction::FullAudit];
        }

        let due_shards = self
            .failed_shards
            .iter()
            .filter_map(|(shard, due_ms)| (*due_ms <= now_ms).then_some((*due_ms, *shard)))
            .take(limit)
            .collect::<Vec<_>>();
        let mut actions = Vec::with_capacity(due_shards.len());
        for (_, shard) in due_shards {
            self.failed_shards
                .insert(shard, now_ms.saturating_add(RECOVERY_POLL_INTERVAL_MS));
            actions.push(PollAction::RecoverShard { shard });
        }
        actions
    }
}
