//! The persisted control mode.
//!
//! The mode is a setting, not a flag: it lives in the ledger (`control.mode`) and every change goes
//! through [`save`], so a restart never silently resumes `auto` and an operator can see who ran in
//! which mode and when. `dry_run` is the default and an unknown stored value falls back to it.

use energy_ledger::{Ledger, LedgerError};

use crate::safety::Authority;

/// Settings key holding the control mode.
pub const MODE_KEY: &str = "control.mode";

/// The stored mode, or `dry_run` when nothing (or something unrecognised) is stored.
pub fn load(ledger: &Ledger) -> Result<Authority, LedgerError> {
    let value = ledger.settings()?.get(MODE_KEY)?;
    Ok(parse(value.as_deref()))
}

/// Persist a mode change.
pub fn save(ledger: &Ledger, mode: Authority, at: i64) -> Result<(), LedgerError> {
    ledger.settings()?.set(MODE_KEY, mode.slug(), at)
}

/// Parse a stored slug; anything unknown is the safe default.
pub fn parse(value: Option<&str>) -> Authority {
    match value {
        Some("auto") => Authority::Auto,
        Some("advisory") => Authority::Advisory,
        _ => Authority::DryRun,
    }
}
