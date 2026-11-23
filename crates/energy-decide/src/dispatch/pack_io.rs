//! Provisional hardware port: M4 receives this from the caller, never creates I/O.

use crate::dispatch::{FlowReport, PackCommand, WriteAck};

/// Bounded, cancellable pack transport and its read/write transaction.
///
/// The coordinator owns the transaction id and releases it at the write's absolute
/// deadline even if a worker is still inside `write`. Implementations must honour
/// `deadline_ms` and `cancel`: a cancelled write must return promptly, and a write
/// that ignores cancellation must still be bounded. Late completions from a
/// superseded generation are ignored by M4.
pub trait PackIo: Send + Sync + 'static {
    /// Acquire the per-pack read/write transaction; exactly one release is required.
    fn begin(&self, pack_id: &str) -> u64;

    /// Release a transaction acquired by `begin`, including after a deadline.
    fn release(&self, pack_id: &str, tx_id: u64);

    /// Send one command under the given transaction and remaining budget in ms.
    fn write(&self, pack_id: &str, command: &PackCommand, deadline_ms: u64, tx_id: u64)
    -> WriteAck;

    /// Stop an in-flight write at its deadline; must be safe to call repeatedly.
    fn cancel(&self, pack_id: &str, tx_id: u64);

    /// Read one flow report within the given deadline.
    fn read(&self, pack_id: &str, deadline_ms: u64) -> Option<FlowReport>;
}
