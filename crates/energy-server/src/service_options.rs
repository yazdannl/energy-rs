//! How the running server is configured.

use std::time::Duration;

/// The knobs that are not part of the engine contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceOptions {
    /// Interval between SSE heartbeat events. Fifteen seconds survives every proxy in front of the
    /// add-on; tests use a few milliseconds.
    pub heartbeat: Duration,
}

impl Default for ServiceOptions {
    fn default() -> Self {
        Self {
            heartbeat: Duration::from_secs(15),
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
}
