//! The fleet-level dispatch row for charge the device capabilities could not absorb.

use energy_ledger::DispatchEntry;

use crate::dispatch::{AppliedPowerReason, TickOutcome};
use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::safety::{Authority, Reason};

/// Journal a typed capability shortfall, never a pack measurement or a write fault.
pub(super) fn record(
    engine: &Engine,
    outcome: &TickOutcome,
    mode: Authority,
    now: i64,
) -> Result<(), EngineError> {
    if outcome.capability_shortfall_w <= 0.0 {
        return Ok(());
    }
    let entry = DispatchEntry::new(
        now,
        None,
        outcome.capability_shortfall_w,
        Reason::ChargeCapabilityShortfall.as_str(),
        mode.slug(),
    )
    .written(false)
    .with_applied_w_reason(AppliedPowerReason::CapabilityShortfall.as_str())
    .with_note(format!(
        "{:.1} W unplaced after ceiling skips: {}",
        outcome.capability_shortfall_w,
        outcome.capability_skipped.join(", ")
    ));
    engine.ledger.dispatch()?.insert(&entry)?;
    Ok(())
}
