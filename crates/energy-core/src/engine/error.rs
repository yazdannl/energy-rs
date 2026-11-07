//! The engine's typed failures.

use energy_ledger::LedgerError;
use thiserror::Error;

use crate::dispatch::FleetError;

/// Everything that stops the engine from completing a tick.
///
/// A refused plan is *not* an error: it is a hold with a journal entry. These are the failures that
/// mean the loop could not even decide.
#[derive(Debug, Error)]
pub enum EngineError {
    /// The ledger refused a read or write.
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    /// The fleet could not be read.
    #[error(transparent)]
    Fleet(#[from] FleetError),
    /// A mutex was poisoned by a panicking holder.
    #[error("engine state is poisoned")]
    Poisoned,
    /// The wiring (measurements, sessions, learning, protection) failed.
    #[error(transparent)]
    Wiring(#[from] crate::wiring::WiringError),
    /// One or more holds failed, after every target was still attempted. The packs that could not be
    /// neutralised keep their latch entry: ownership is never dropped by a bookkeeping failure.
    #[error("hold failed for {failed} pack(s): {detail}")]
    Holds {
        /// How many holds could not be completed.
        failed: usize,
        /// One `pack: cause` line per failure.
        detail: String,
    },
    /// The persisted live-write latch could not be parsed, so what a previous run left on the
    /// hardware is unknown. The raw value is left untouched in the ledger (`control.live_writes`)
    /// and `auto` control is refused until an operator reconciles it.
    #[error(
        "the persisted live-write latch is unreadable ({bytes} bytes); \
         auto control is refused until it is reconciled"
    )]
    LatchCorrupt {
        /// Size of the stored value, so the log can identify the row without echoing it.
        bytes: usize,
    },
    /// The live-write latch could not be serialised, so nothing was written over the stored value.
    #[error("the live-write latch could not be serialised: {reason}")]
    LatchWrite {
        /// Serialiser cause.
        reason: String,
    },
}
