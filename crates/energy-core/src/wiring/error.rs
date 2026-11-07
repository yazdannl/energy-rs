//! Every way the wiring helpers can fail.

use energy_battery::BatteryError;
use energy_ledger::LedgerError;
use thiserror::Error;

/// A measured/learned/protected state update that could not be persisted or computed.
#[derive(Debug, Error)]
pub enum WiringError {
    /// The ledger refused a read or write.
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    /// The learning or protection crate refused its input.
    #[error(transparent)]
    Battery(#[from] BatteryError),
}
