use std::collections::VecDeque;

const MIN_REQUEST_INTERVAL_MS: u64 = 800;
const MIN_RESPONSE_INTERVAL_MS: u64 = 2_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenewalRateGate {
    max_requests: usize,
    request_times_ms: VecDeque<u64>,
}

impl RenewalRateGate {
    pub fn per_second(max_requests: usize) -> Self {
        assert!(max_requests > 0, "renewal rate must be positive");
        Self {
            max_requests,
            request_times_ms: VecDeque::with_capacity(max_requests),
        }
    }

    pub fn try_take(&mut self, now_ms: u64) -> bool {
        while self
            .request_times_ms
            .front()
            .is_some_and(|oldest| now_ms.saturating_sub(*oldest) >= 1_000)
        {
            self.request_times_ms.pop_front();
        }
        if self.request_times_ms.len() >= self.max_requests {
            return false;
        }
        self.request_times_ms.push_back(now_ms);
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenewalBatchGate {
    next_due_ms: u64,
}

impl RenewalBatchGate {
    pub fn new(start_ms: u64) -> Self {
        Self {
            next_due_ms: start_ms.saturating_add(MIN_RESPONSE_INTERVAL_MS),
        }
    }

    pub fn take_due(&mut self, now_ms: u64) -> bool {
        if now_ms < self.next_due_ms {
            return false;
        }
        self.next_due_ms = now_ms.saturating_add(MIN_RESPONSE_INTERVAL_MS);
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenewalRequestItem {
    pub market: u8,
    pub code: String,
    pub token: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenewalSecurity {
    market: u8,
    code: String,
    last_request_ms: u64,
    last_response_ms: u64,
    token: u32,
}

impl RenewalSecurity {
    pub fn new(
        market: u8,
        code: impl Into<String>,
        last_request_ms: u64,
        last_response_ms: u64,
    ) -> Self {
        Self {
            market,
            code: code.into(),
            last_request_ms,
            last_response_ms,
            token: 0,
        }
    }

    fn eligible_at_ms(&self) -> u64 {
        self.last_request_ms
            .saturating_add(MIN_REQUEST_INTERVAL_MS)
            .max(
                self.last_response_ms
                    .saturating_add(MIN_RESPONSE_INTERVAL_MS),
            )
    }
}

#[derive(Clone, Debug, Default)]
pub struct QuoteRenewalScheduler {
    securities: Vec<RenewalSecurity>,
}

impl QuoteRenewalScheduler {
    pub fn new(securities: impl IntoIterator<Item = RenewalSecurity>) -> Self {
        Self {
            securities: securities.into_iter().collect(),
        }
    }

    pub fn take_due(&mut self, now_ms: u64, limit: usize) -> Vec<RenewalRequestItem> {
        let mut due = self
            .securities
            .iter()
            .enumerate()
            .filter_map(|(index, security)| {
                let eligible_at_ms = security.eligible_at_ms();
                (eligible_at_ms <= now_ms).then_some((
                    eligible_at_ms,
                    security.market,
                    security.code.clone(),
                    index,
                ))
            })
            .collect::<Vec<_>>();
        due.sort_unstable();

        due.into_iter()
            .take(limit)
            .map(|(_, _, _, index)| {
                let security = &mut self.securities[index];
                security.last_request_ms = now_ms;
                RenewalRequestItem {
                    market: security.market,
                    code: security.code.clone(),
                    token: security.token,
                }
            })
            .collect()
    }

    pub fn has_due(&self, now_ms: u64) -> bool {
        self.securities
            .iter()
            .any(|security| security.eligible_at_ms() <= now_ms)
    }

    pub fn record_response(&mut self, market: u8, code: &str, token: u32, now_ms: u64) {
        if let Some(security) = self
            .securities
            .iter_mut()
            .find(|security| security.market == market && security.code == code)
        {
            security.token = token;
            security.last_response_ms = now_ms;
        } else {
            self.securities.push(RenewalSecurity {
                market,
                code: code.to_string(),
                last_request_ms: 0,
                last_response_ms: now_ms,
                token,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{QuoteRenewalScheduler, RenewalBatchGate, RenewalRateGate, RenewalSecurity};

    #[test]
    fn global_renewal_rate_gate_enforces_a_sliding_one_second_ceiling() {
        let mut gate = RenewalRateGate::per_second(160);

        for _ in 0..160 {
            assert!(gate.try_take(0));
        }
        assert!(!gate.try_take(999));
        assert!(gate.try_take(1_000));
    }

    #[test]
    fn rejected_renewal_does_not_consume_the_next_global_slot() {
        let mut gate = RenewalRateGate::per_second(1);

        assert!(gate.try_take(0));
        assert!(!gate.try_take(999));
        assert!(gate.try_take(1_000));
    }

    #[test]
    fn shard_batch_gate_allows_at_most_one_renewal_every_two_seconds() {
        let mut gate = RenewalBatchGate::new(0);

        assert!(!gate.take_due(1_999));
        assert!(gate.take_due(2_000));
        assert!(!gate.take_due(2_000));
        assert!(!gate.take_due(3_999));
        assert!(gate.take_due(4_000));
    }

    #[test]
    fn renewal_requires_both_vendor_observed_time_gates() {
        let mut scheduler = QuoteRenewalScheduler::new([
            RenewalSecurity::new(0, "000001", 1_000, 1_000),
            RenewalSecurity::new(1, "600000", 0, 1_500),
            RenewalSecurity::new(1, "600001", 2_500, 0),
        ]);

        assert!(scheduler.take_due(1_799, 100).is_empty());
        assert_eq!(
            scheduler
                .take_due(3_000, 100)
                .into_iter()
                .map(|item| (item.market, item.code))
                .collect::<Vec<_>>(),
            vec![(0, "000001".to_string())]
        );
    }

    #[test]
    fn bounded_batches_do_not_starve_other_due_securities() {
        let mut scheduler = QuoteRenewalScheduler::new([
            RenewalSecurity::new(0, "000001", 0, 0),
            RenewalSecurity::new(1, "600000", 0, 0),
        ]);

        assert_eq!(scheduler.take_due(2_000, 1)[0].code, "000001");
        assert_eq!(scheduler.take_due(2_000, 1)[0].code, "600000");
    }

    #[test]
    fn response_rearms_security_with_latest_vendor_token() {
        let mut scheduler = QuoteRenewalScheduler::new([RenewalSecurity::new(1, "600000", 0, 0)]);

        scheduler.record_response(1, "600000", 0x1234_5678, 1_000);

        assert!(scheduler.take_due(2_999, 100).is_empty());
        let due = scheduler.take_due(3_000, 100);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].token, 0x1234_5678);
    }
}
