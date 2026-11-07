//! Register packs in the ledger and resolve their ids.

use std::collections::BTreeMap;

use energy_ledger::{Ledger, LedgerError, Pack};

use crate::dispatch::FleetView;

/// Give every pack a ledger id, registering the ones seen for the first time.
///
/// The control loop identifies a pack by its serial, the ledger by a row id, and every fleet read
/// starts with no id; `known` caches the mapping for the life of the process so a tick does not
/// re-upsert the fleet. Only the first sight of a pack writes: `last_seen` and the reported capacity
/// are updated by measurement ingestion, not by every tick.
pub(crate) fn resolve(
    ledger: &Ledger,
    view: &mut FleetView,
    now: i64,
    known: &mut BTreeMap<String, i64>,
) -> Result<(), LedgerError> {
    for reading in &mut view.packs {
        if let Some(id) = known.get(reading.pack_id()) {
            reading.ledger_id = Some(*id);
            continue;
        }
        let row = Pack::new(reading.pack_id(), reading.spec.capacity_kwh, now)
            .with_limits(reading.spec.max_charge_w, reading.spec.max_discharge_w);
        let stored = ledger.packs()?.upsert(&row)?;
        known.insert(reading.pack_id().to_owned(), stored.id);
        reading.ledger_id = Some(stored.id);
    }
    Ok(())
}
