//! One open fault.

/// A fault the engine is holding against: HA link, battery transport, stale input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaultSnapshot {
    /// Stable machine kind (`ha_disconnected`, `battery_unreachable`, `stale_forecast`, …).
    pub kind: String,
    /// Short human-readable description; never contains a token or a file path.
    pub message: String,
    /// When the fault opened, UTC epoch seconds.
    pub since: i64,
}
