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
}
