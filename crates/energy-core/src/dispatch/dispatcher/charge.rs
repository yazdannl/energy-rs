//! Device-reported charge ceiling lockout, one dispatcher state per pack.

use std::sync::PoisonError;

use crate::allocation::ChargeSkip;
use crate::dispatch::TickInput;

use super::Dispatcher;

/// Fold the device reports into the charge lockout, then name every skipped pack.
///
/// A closed device latches immediately; it is released only when a fresh report shows its SoC
/// past [`crate::allocation::CHARGE_RELEASE_HYSTERESIS_PCT`] below the ceiling. A stale sample
/// never releases the latch, and never authorises a charge either.
pub(super) fn skips(dispatcher: &Dispatcher, input: &TickInput<'_>) -> Vec<ChargeSkip> {
    let mut latched = dispatcher
        .charge_latch
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut skips = Vec::new();
    for reading in &input.fleet.packs {
        let pack_id = reading.pack_id();
        // Only a fresh, usable report may newly close a pack. Staleness retains an existing latch
        // but never creates one: a later fresh 99 % report must remain chargeable.
        if reading.usable() && reading.device_charge_closed() {
            latched.insert(pack_id.to_owned());
        } else if reading.device_charge_released() {
            latched.remove(pack_id);
        }
        if let Some(skip) = reading.charge_skip(latched.contains(pack_id)) {
            skips.push(skip);
        }
    }
    skips
}
