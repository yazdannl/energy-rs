//! One-minute measurement ingestion and the 5-minute rollup.

use energy_ledger::{Ledger, LedgerError, Sample};

use crate::dispatch::FleetView;

/// Write one sample per pack for the current minute, then roll the last five minutes up.
///
/// The tick is 5 s, the measurement tier is one minute: the first tick of a new minute writes, the
/// rest are no-ops. Ingestion is an upsert keyed `(pack_id, ts)`, so a retried tick cannot duplicate
/// a row, and a missing SoC is stored as `NULL` instead of being interpolated.
pub(crate) fn record(
    ledger: &Ledger,
    now: i64,
    view: &FleetView,
    last_minute: &mut Option<i64>,
) -> Result<usize, LedgerError> {
    let minute = now.div_euclid(60);
    if *last_minute == Some(minute) {
        return Ok(0);
    }
    let samples: Vec<Sample> = view
        .packs
        .iter()
        .filter_map(|pack| {
            let pack_id = pack.ledger_id?;
            Some(Sample::new(pack_id, now, pack.power_w, pack.soc_pct))
        })
        .collect();
    if samples.is_empty() {
        return Ok(0);
    }
    let written = ledger.measurements()?.write_many(&samples)?;
    ledger
        .measurements()?
        .rollup(now.saturating_sub(300), now.saturating_add(1))?;
    *last_minute = Some(minute);
    Ok(written)
}
