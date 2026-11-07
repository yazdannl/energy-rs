//! Plan inputs from what the doctor can see: live device reports first, the ledger's last samples
//! second.
//!
//! The runtime's `PackCatalog` registers packs (a write); a diagnostic must not, so the same
//! [`PackSpec`] fields are assembled here from the ledger rows and the device reports. Capacity and
//! efficiency prefer the learned values, then the label, then the typed round trip — the same order
//! the runtime uses.

use std::collections::BTreeMap;

use energy_battery::fleet::DeviceSnapshot;
use energy_battery::learning::DEFAULT_ROUND_TRIP;
use energy_battery::report::Report;
use energy_core::plan::{PackSpec, PackState};
use energy_ledger::{Ledger, Pack};

/// One `PackState` per pack that has a usable SoC, devices before ledger-only packs.
pub fn build(ledger: Option<&Ledger>, devices: &[DeviceSnapshot]) -> Vec<PackState> {
    let packs = ledger_packs(ledger);
    let mut states: Vec<PackState> = Vec::new();
    for device in devices {
        let Some(report) = device.report.as_ref() else {
            continue;
        };
        let row = packs.get(report.sn.as_str()).cloned();
        let Some(spec) = spec(report.sn.clone(), row.as_ref(), Some(report)) else {
            continue;
        };
        states.push(PackState {
            spec,
            soc_pct: report.soc_pct(),
        });
    }
    if let Some(ledger) = ledger {
        ledger_only(ledger, &mut states);
    }
    states
}

fn ledger_packs(ledger: Option<&Ledger>) -> BTreeMap<String, Pack> {
    let Some(ledger) = ledger else {
        return BTreeMap::new();
    };
    ledger
        .packs()
        .and_then(|packs| packs.all())
        .map(|packs| {
            packs
                .into_iter()
                .map(|pack| (pack.sn.clone(), pack))
                .collect()
        })
        .unwrap_or_default()
}

/// Packs with no live report: the newest ledger sample is the SoC.
fn ledger_only(ledger: &Ledger, states: &mut Vec<PackState>) {
    let Ok(stored) = ledger.packs().and_then(|packs| packs.states()) else {
        return;
    };
    for state in stored {
        if states
            .iter()
            .any(|known| known.spec.pack_id == state.pack.sn)
        {
            continue;
        }
        let Some(sample) = state.last else {
            continue;
        };
        let Some(soc_pct) = sample.soc_pct.filter(|soc| soc.is_finite()) else {
            continue;
        };
        if let Some(spec) = spec(state.pack.sn.clone(), Some(&state.pack), None) {
            states.push(PackState { spec, soc_pct });
        }
    }
}

/// The spec of one pack, preferring learned values and falling back in the documented order.
fn spec(pack_id: String, row: Option<&Pack>, report: Option<&Report>) -> Option<PackSpec> {
    let label = report
        .and_then(Report::total_capacity_kwh)
        .or_else(|| row.map(|pack| pack.label_capacity_kwh))?;
    let capacity_kwh = row.and_then(effective_capacity).unwrap_or(label).max(0.001);
    let round_trip = row
        .and_then(|pack| pack.learned_round_trip)
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 1.0)
        .unwrap_or(DEFAULT_ROUND_TRIP);
    let eta = round_trip.sqrt();
    let charge_w = row
        .and_then(|pack| pack.max_charge_w)
        .filter(|watts| *watts > 0.0)
        .unwrap_or_else(|| {
            f64::from(
                report
                    .map_or(0, |report| report.properties.charge_max_w)
                    .max(800),
            )
        });
    let discharge_w = row
        .and_then(|pack| pack.max_discharge_w)
        .filter(|watts| *watts > 0.0)
        .unwrap_or_else(|| {
            f64::from(
                report
                    .map_or(0, |report| report.properties.inverse_max_w)
                    .max(800),
            )
        });
    let spec = PackSpec {
        pack_id,
        capacity_kwh,
        min_soc_pct: 0.0,
        max_soc_pct: 100.0,
        max_charge_w: charge_w,
        max_discharge_w: discharge_w,
        eta_charge: eta,
        eta_discharge: eta,
    };
    spec.invalid().is_none().then_some(spec)
}

fn effective_capacity(pack: &Pack) -> Option<f64> {
    pack.learned_capacity_kwh
        .filter(|value| value.is_finite() && *value > 0.0)
        .or_else(|| {
            pack.reported_capacity_kwh
                .filter(|value| value.is_finite() && *value > 0.0)
        })
}
