//! Bounded safety-lane retry and escalation record.

/// Retry offsets after a failed safety attempt, epoch-relative milliseconds.
pub const SAFETY_RETRY_OFFSETS_MS: [u64; 3] = [0, 2_000, 6_000];
/// Total bounded safety attempt: 2 s ack + 60 s ramp + 12 s receipt.
pub const SAFETY_ATTEMPT_DEADLINE_MS: u64 = 74_000;

/// One escalation path, with every step recorded in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escalation {
    /// Named path, for example `ownership_retention`.
    pub path: &'static str,
    /// Ordered steps taken so far.
    pub steps: Vec<&'static str>,
    /// Next permitted attempt, epoch milliseconds.
    pub retry_at_ms: Option<i64>,
    /// Number of failed attempts so far.
    pub failures: u8,
}

impl Escalation {
    /// Fresh escalation record with no failed attempts.
    pub fn new(path: &'static str) -> Self {
        Self {
            path,
            steps: Vec::new(),
            retry_at_ms: None,
            failures: 0,
        }
    }

    /// Record a failed attempt and schedule the next one from elapsed time, not a sleep.
    pub fn fail(&mut self, at_ms: i64, reason: &'static str) {
        self.failures = self.failures.saturating_add(1);
        self.steps.push(reason);
        let index = usize::from(self.failures.saturating_sub(1));
        self.retry_at_ms = SAFETY_RETRY_OFFSETS_MS
            .get(index.saturating_add(1))
            .map(|offset| at_ms.saturating_add(i64::try_from(*offset).unwrap_or(i64::MAX)));
        if self.failures >= SAFETY_RETRY_OFFSETS_MS.len() as u8 {
            self.steps.push("safety_write_unverified");
            self.steps.push(self.path);
            self.retry_at_ms = None;
        }
    }

    /// Whether three bounded attempts have failed.
    pub fn exhausted(&self) -> bool {
        self.failures >= SAFETY_RETRY_OFFSETS_MS.len() as u8
    }
}
