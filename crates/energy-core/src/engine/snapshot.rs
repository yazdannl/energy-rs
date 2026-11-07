//! The engine's read model: one [`EngineSnapshot`] per tick, published to the HTTP layer.
//!
//! The snapshot is assembled from state the engine already holds: the tick's fleet view and meter,
//! the plan in force, the per-pack learned profile and protection latch, and the open faults. The
//! server adds discovery through [`RuntimeView`]. Publishing is a plain value onto a `watch`
//! channel, so it never blocks a tick and a resting UI costs nothing.

use std::sync::Arc;

use energy_battery::learning::{LearningProfile, LearningStore};
use energy_battery::protection::{CellLatch, CellLatchStore};

use crate::dispatch::{FleetView, ManualOverride, MeterReading};
use crate::engine::control::Engine;
use crate::engine::faults;
use crate::plan::Plan;
use crate::stream::{
    EngineSnapshot, ForecastSnapshot, MeterSnapshot, PackSnapshot, PlanSnapshot, RuntimeView,
    SnapshotPublisher,
};

/// Forecast energy in one 15-minute slot, Wh (1 kW for 900 s).
const SLOT_WH_PER_KW: f64 = 250.0;

impl Engine {
    /// Attach the API side: where snapshots go and what the server discovered.
    #[must_use]
    pub fn with_stream(
        mut self,
        publisher: Arc<dyn SnapshotPublisher>,
        runtime: Arc<dyn RuntimeView>,
    ) -> Self {
        self.publisher = Some(publisher);
        self.runtime = Some(runtime);
        self
    }

    /// Build the read model for this instant without touching the channel.
    ///
    /// A poisoned state mutex yields the empty snapshot: a reader sees "no engine state" instead of
    /// a torn mix, and the supervisor's watchdog is what reports the stall.
    pub fn snapshot(&self, now: i64, view: &FleetView, meter: &MeterReading) -> EngineSnapshot {
        let Ok(state) = self.lock() else {
            return EngineSnapshot::default();
        };
        let mode = state.mode;
        let plan = state.plan.clone();
        let plan_id = state.plan_id;
        let overrides: Vec<ManualOverride> = state
            .overrides
            .iter()
            .filter(|entry| entry.active(now))
            .cloned()
            .collect();
        let last_tick = state.watchdog.last_progress();
        drop(state);

        let packs = self.pack_snapshots(view, now, &overrides);
        let plan_snapshot = plan.as_ref().map(|plan| plan_snapshot(plan, plan_id));
        let faults = faults::derive(now, plan.as_ref(), view, meter, self.runtime.as_deref());
        EngineSnapshot {
            mode,
            site: self
                .runtime
                .as_ref()
                .map_or_else(Default::default, |runtime| runtime.site()),
            plan: plan_snapshot,
            packs,
            meter: meter_snapshot(now, view, meter),
            forecast: plan.as_ref().map(forecast_snapshot),
            last_tick,
            faults,
        }
    }

    /// Publish the state known at boot: the persisted mode, discovery and the faults before the
    /// first tick. `last_tick` stays empty until a tick really completed.
    pub(crate) fn publish_boot(&self, now: i64) {
        let Some(publisher) = &self.publisher else {
            return;
        };
        let Ok(state) = self.lock() else {
            return;
        };
        let mode = state.mode;
        drop(state);
        let view = FleetView::default();
        let meter = MeterReading::default();
        publisher.publish(EngineSnapshot {
            mode,
            site: self
                .runtime
                .as_ref()
                .map_or_else(Default::default, |runtime| runtime.site()),
            faults: faults::derive(now, None, &view, &meter, self.runtime.as_deref()),
            ..EngineSnapshot::default()
        });
    }

    /// Publish the model after a tick. A server without a publisher simply skips this.
    pub(crate) fn publish_tick(&self, now: i64, view: &FleetView, meter: &MeterReading) {
        if let Some(publisher) = &self.publisher {
            publisher.publish(self.snapshot(now, view, meter));
        }
    }

    /// Publish a command's effect immediately, by patching the current snapshot's mode and
    /// overrides. The next tick replaces it with a fully re-read one.
    pub(crate) fn publish_meta(&self, now: i64) {
        let Some(publisher) = &self.publisher else {
            return;
        };
        let mut snapshot = publisher.current();
        let Ok(state) = self.lock() else {
            return;
        };
        snapshot.mode = state.mode;
        snapshot.packs = patch_overrides(snapshot.packs, &state.overrides, now);
        drop(state);
        publisher.publish(snapshot);
    }

    /// One snapshot entry per pack, with the learned profile and the protection latch of that pack.
    fn pack_snapshots(
        &self,
        view: &FleetView,
        now: i64,
        overrides: &[ManualOverride],
    ) -> Vec<PackSnapshot> {
        view.packs
            .iter()
            .map(|pack| {
                let sn = pack.pack_id();
                PackSnapshot {
                    sn: sn.to_owned(),
                    soc_pct: pack.soc_pct,
                    power_w: Some(pack.power_w),
                    sampled_at: Some(pack.observed_at),
                    stale: pack.stale || !pack.available,
                    max_charge_w: Some(pack.spec.max_charge_w),
                    max_discharge_w: Some(pack.spec.max_discharge_w),
                    profile: profile(self.learning_store.load(sn)),
                    latch: latch(self.cell_store.load(sn)),
                    override_state: overrides
                        .iter()
                        .find(|entry| entry.pack_id == sn && entry.active(now))
                        .cloned(),
                }
            })
            .collect()
    }
}

/// A stored profile, with a read failure logged and treated as "never learned".
fn profile(
    result: Result<Option<LearningProfile>, energy_battery::BatteryError>,
) -> Option<LearningProfile> {
    match result {
        Ok(profile) => profile,
        Err(error) => {
            tracing::warn!(%error, "learning profile read failed for the snapshot");
            None
        }
    }
}

/// A stored latch, with a read failure logged and treated as "no latch".
fn latch(result: Result<Option<CellLatch>, energy_battery::BatteryError>) -> Option<CellLatch> {
    match result {
        Ok(latch) => latch,
        Err(error) => {
            tracing::warn!(%error, "cell latch read failed for the snapshot");
            None
        }
    }
}

/// Replace the override fields without re-reading the fleet.
fn patch_overrides(
    packs: Vec<PackSnapshot>,
    overrides: &[ManualOverride],
    now: i64,
) -> Vec<PackSnapshot> {
    packs
        .into_iter()
        .map(|mut pack| {
            pack.override_state = overrides
                .iter()
                .find(|entry| entry.pack_id == pack.sn && entry.active(now))
                .cloned();
            pack
        })
        .collect()
}

/// The plan header the API renders.
fn plan_snapshot(plan: &Plan, plan_id: Option<i64>) -> PlanSnapshot {
    PlanSnapshot {
        plan_id: plan_id.unwrap_or(0),
        generated_at: plan.generated_at,
        horizon_start: plan.horizon_start,
        horizon_end: plan.horizon_end,
        feasible: plan.feasible,
        summary: plan.summary.clone(),
        binding_reasons: binding_reasons(plan),
    }
}

/// The distinct slot rationales of the plan, in timeline order, plus a degraded-forecast note.
fn binding_reasons(plan: &Plan) -> Vec<String> {
    let mut reasons: Vec<String> = Vec::new();
    for slot in &plan.slots {
        if !slot.reason.trim().is_empty() && !reasons.contains(&slot.reason) {
            reasons.push(slot.reason.clone());
        }
    }
    if let Some(note) = &plan.degraded
        && !reasons.iter().any(|reason| reason == note)
    {
        reasons.push(note.clone());
    }
    reasons
}

/// The forecast totals the plan was solved on, summed from its own slots.
fn forecast_snapshot(plan: &Plan) -> ForecastSnapshot {
    ForecastSnapshot {
        at: plan.generated_at,
        horizon_start: plan.horizon_start,
        pv_wh: plan.slots.iter().map(|slot| slot.pv_kw).sum::<f64>() * SLOT_WH_PER_KW,
        load_wh: plan.slots.iter().map(|slot| slot.load_kw).sum::<f64>() * SLOT_WH_PER_KW,
        confidence: None,
    }
}

/// The live meter reading, including the fleet's measured AC power.
fn meter_snapshot(now: i64, view: &FleetView, meter: &MeterReading) -> Option<MeterSnapshot> {
    let grid_w = meter.grid_w?;
    Some(MeterSnapshot {
        at: now,
        grid_w,
        pv_w: meter.pv_kw.map_or(0.0, |pv| (pv * 1000.0).max(0.0)),
        load_w: meter.load_kw.map_or(0.0, |load| (load * 1000.0).max(0.0)),
        battery_w: Some(view.power_w()),
    })
}
