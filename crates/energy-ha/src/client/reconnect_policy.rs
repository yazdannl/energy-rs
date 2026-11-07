//! Exponential reconnect backoff.

use std::time::Duration;

/// Exponential backoff between WebSocket (re)connects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectPolicy {
    pub initial: Duration,
    pub max: Duration,
    pub jitter: bool,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
            jitter: true,
        }
    }
}

impl ReconnectPolicy {
    /// Delay before retry `attempt` (zero based).
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 2u32.saturating_pow(attempt.min(10));
        let base = self.initial.saturating_mul(factor).min(self.max);
        let base = base.max(Duration::from_millis(1));
        if !self.jitter {
            return base;
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.subsec_nanos());
        // 0.8..=1.2 multiplier, no extra dependency.
        let factor = 0.8 + f64::from(nanos % 401) / 1000.0;
        base.mul_f64(factor).min(self.max)
    }
}
