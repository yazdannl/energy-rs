//! The durable write-intent journal a gated sink consults before it touches hardware.

use crate::dispatch::SinkError;

/// Records, durably, that a setpoint is about to be written to a pack.
///
/// This is the crash boundary: a command that reached a device but was never journalled leaves
/// untracked hardware after a restart, so the journal write must succeed *before* the transport
/// write. A journal that refuses means the command is not sent at all
/// ([`SinkError::NotOwned`]).
pub trait WriteIntent: Send + Sync {
    /// Claim ownership of `pack_id` at `watts`, durable before the caller returns.
    fn claim(&self, pack_id: &str, watts: f64, now: i64) -> Result<(), SinkError>;
}
