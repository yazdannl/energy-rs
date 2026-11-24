//! Provisional hardware port: M4 receives this from the caller, never creates I/O.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::dispatch::{FlowReport, PackCommand, WriteAck};

/// Bounded, cancellable pack transport and its read/write transaction.
///
/// M4's arbiter owns one future per pack. The future's return value is the acknowledgement;
/// there is no channel and no second claimant. Once a request has left for the device it
/// cannot be un-sent: cancellation and generation fencing prevent STATE contamination only.
/// A cancelled write that ignores the token stays unresolved until reconciliation joins it.
/// Physical application MAY occur after `write` returns; M4 assumes yes and requires
/// evidence strictly after the future is joined before clearing uncertainty.
pub trait PackIo: Send + Sync + 'static {
    /// Acquire the per-pack read/write transaction; exactly one release is required.
    fn begin(&self, pack_id: &str) -> u64;

    /// Release a transaction acquired by `begin`, including after a deadline.
    /// This call MAY block on locks held by a stuck write; the arbiter never calls it.
    /// Each pack's write guard releases its own transaction, so a slow release costs
    /// only that pack. Implementations must terminate.
    fn release(&self, pack_id: &str, tx_id: u64);

    /// Send one command under this transaction. The future token is set at the deadline;
    /// implementations must check it and return promptly when cancellation is requested.
    fn write(
        &self,
        pack_id: &str,
        command: &PackCommand,
        deadline_ms: u64,
        tx_id: u64,
        cancel: Arc<AtomicBool>,
    ) -> WriteAck;

    /// Read one flow report within the given deadline.
    fn read(&self, pack_id: &str, deadline_ms: u64) -> Option<FlowReport>;
}

/// Whether a cancellation token has been set.
pub fn cancelled(token: &Arc<AtomicBool>) -> bool {
    token.load(Ordering::SeqCst)
}
