//! Savings accounting failures.

use energy_ledger::LedgerError;

/// Why a savings day could not be read or stored.
#[derive(Debug, thiserror::Error)]
pub enum SavingsError {
    /// The ledger refused a read or a write.
    #[error("ledger: {0}")]
    Ledger(#[from] LedgerError),
    /// The price store refused a range read.
    #[error("prices: {0}")]
    Prices(String),
    /// A required dashboard statistic is not configured or could not be read.
    #[error("measured series {series} is unavailable: {reason}")]
    Measurement {
        /// Statistic role (`grid_import`, `grid_export`, `solar_energy`, …).
        series: &'static str,
        /// What went wrong (a Home Assistant error, never a secret).
        reason: String,
    },
    /// The energy dashboard is not configured, so nothing can be measured.
    #[error("the Home Assistant energy dashboard is not configured")]
    SiteNotConfigured,
    /// An instant outside the representable range (a corrupt clock or ledger key).
    #[error("an instant used for a savings day is outside the representable range")]
    Clock,
}
