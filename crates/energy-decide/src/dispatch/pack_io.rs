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
/// Hard contract: `write` MUST return success, refusal or error within `TRANSPORT_BOUND_MS`
/// and MUST be unwind-safe. A write that blocks forever is a CONTRACT VIOLATION: M4 latches
/// that pack and reports it, but cannot release a transaction the consumer never returns.
/// Ordered-application contract: the pack applies commands in the order it receives them
/// (single-threaded firmware, one command per pack at a time). A device-generated report
/// timestamped after the neutralising command's acknowledgement therefore reflects a state
/// in which every earlier command was applied or superseded. If the consumer cannot declare
/// this, automatic recovery is unsupported and the pack needs an explicit reconcile.
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
    /// Must return within `TRANSPORT_BOUND_MS` and unwind safely on every path.
    fn write(
        &self,
        pack_id: &str,
        command: &PackCommand,
        deadline_ms: u64,
        tx_id: u64,
        cancel: Arc<AtomicBool>,
    ) -> WriteAck;

    /// Whether this transport guarantees ordered physical application per pack.
    /// There is deliberately no default: every implementation must answer.
    fn ordered_application(&self) -> bool;

    /// Read one flow report within the given deadline.
    fn read(&self, pack_id: &str, deadline_ms: u64) -> Option<FlowReport>;
}

/// Whether a cancellation token has been set.
pub fn cancelled(token: &Arc<AtomicBool>) -> bool {
    token.load(Ordering::SeqCst)
}
