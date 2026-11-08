//! The faults a snapshot carries: what the engine itself knows is wrong, plus the server's own.
//!
//! A fault is opened here and cleared here (the reader only renders it), so the UI never has to
//! infer health from missing data. Kinds are stable slugs; messages never contain a token, a URL or
//! a file path.

use crate::dispatch::{FleetView, MeterReading};
use crate::engine::Refusal;
use crate::plan::Plan;
use crate::stream::{FaultSnapshot, RuntimeView};

/// Seconds a per-pack fault waits before it is worth surfacing (one stale-read bound).
const PACK_FAULT_AFTER_S: i64 = 60;

/// Everything wrong at `now`, newest last.
pub(crate) fn derive(
    now: i64,
    plan: Option<&Plan>,
    view: &FleetView,
    meter: &MeterReading,
    runtime: Option<&dyn RuntimeView>,
    refusal: Option<&Refusal>,
) -> Vec<FaultSnapshot> {
    let mut faults = Vec::new();
    match plan {
        None => faults.push(FaultSnapshot {
            kind: "no_plan".to_owned(),
            message: "no plan has been generated yet".to_owned(),
            since: now,
        }),
        Some(plan) if now >= plan.horizon_end => faults.push(FaultSnapshot {
            kind: "plan_expired".to_owned(),
            message: format!(
                "the plan ended at {} and has not been replaced",
                plan.horizon_end
            ),
            since: plan.horizon_end,
        }),
        Some(_) => {}
    }
    if let Some(refusal) = refusal {
        faults.push(FaultSnapshot {
            kind: "plan_refused".to_owned(),
            message: format!(
                "the last plan solve was refused, so the packs are held: {}",
                refusal.reason
            ),
            since: refusal.at,
        });
    }
    faults.extend(pack_faults(now, view));
    if meter.grid_w.is_none() {
        faults.push(FaultSnapshot {
            kind: "meter_missing".to_owned(),
            message: "the grid power sensor has no usable reading".to_owned(),
            since: now,
        });
    }
    if let Some(runtime) = runtime {
        faults.extend(runtime.faults());
    }
    faults
}

/// Unreachable and stale packs, after the freshness bound rather than on the first slow poll.
fn pack_faults(now: i64, view: &FleetView) -> Vec<FaultSnapshot> {
    let mut faults = Vec::new();
    for pack in &view.packs {
        if !pack.available {
            faults.push(FaultSnapshot {
                kind: "pack_unreachable".to_owned(),
                message: format!("pack {} did not answer; it is held", pack.pack_id()),
                since: now,
            });
        } else if pack.stale && now.saturating_sub(pack.observed_at) >= PACK_FAULT_AFTER_S {
            faults.push(FaultSnapshot {
                kind: "pack_stale".to_owned(),
                message: format!(
                    "pack {} last reported {} s ago",
                    pack.pack_id(),
                    now.saturating_sub(pack.observed_at)
                ),
                since: pack.observed_at,
            });
        }
    }
    faults
}
