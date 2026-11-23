//! Provisional hardware port: M4 receives this from the caller, never creates I/O.

use crate::dispatch::{FlowReport, PackCommand, WriteAck};

/// Bounded pack transport and its read/write transaction.
///
/// The coordinator owns the transaction id and releases it even when a worker thread
/// overruns the end-to-end budget. Implementations must be safe to share across pack workers.
pub trait PackIo: Send + Sync + 'static {
    /// Acquire the per-pack read/write transaction; exactly one release is required.
    fn begin(&self, pack_id: &str) -> u64;

    /// Release a transaction acquired by `begin`, including after a deadline.
    fn release(&self, pack_id: &str, tx_id: u64);

    /// Send one command; the deadline is the remaining single per-pack budget in ms.
    fn write(&self, pack_id: &str, command: &PackCommand, deadline_ms: u64) -> WriteAck;

    /// Read one flow report within the given deadline.
    fn read(&self, pack_id: &str, deadline_ms: u64) -> Option<FlowReport>;
}
