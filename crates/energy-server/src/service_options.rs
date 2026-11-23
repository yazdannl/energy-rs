//! How the running server is configured.

use std::time::Duration;

/// How long the health endpoint may see no completed tick before it reports degraded.
pub const DEFAULT_HEALTH_STALE_AFTER: Duration = Duration::from_secs(120);

/// No health failure is reported for the first 90 s after boot: discovery, the first fleet read and
/// the first solve are legitimate startup work, not a stalled loop.
pub const HEALTH_STARTUP_GRACE: Duration = Duration::from_secs(90);

/// The knobs that are not part of the engine contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceOptions {
    /// Interval between SSE heartbeat events. Fifteen seconds survives every proxy in front of the
    /// add-on; tests use a few milliseconds.
    pub heartbeat: Duration,
    /// Age at which a missing tick or pack sample makes `/api/v1/health` answer 503.
    pub health_stale_after: Duration,
}

impl Default for ServiceOptions {
    fn default() -> Self {
        Self {
            heartbeat: Duration::from_secs(15),
            health_stale_after: DEFAULT_HEALTH_STALE_AFTER,
        }
    }
}

impl ServiceOptions {
    /// Options with a specific heartbeat interval.
    #[must_use]
    pub fn with_heartbeat(mut self, heartbeat: Duration) -> Self {
        self.heartbeat = heartbeat;
        self
    }

    /// Options with a specific stale-health threshold (tests and operator overrides).
    #[must_use]
    pub fn with_health_stale_after(mut self, stale_after: Duration) -> Self {
        self.health_stale_after = stale_after;
        self
    }
}
