//! The live policy: settings re-read every tick, plus the cost pool advanced from measurements.

use std::sync::Arc;


use crate::dispatch::{FleetView, MeterReading};

impl crate::engine::Engine {
    /// Follow the live policy and advance the cost pool from measured flows.
    #[must_use]
    pub fn with_policy(
        mut self,
        handle: Arc<crate::plan_source::PolicyHandle>,
        source: Arc<dyn crate::plan_source::PolicySource>,
        pool: Arc<dyn crate::savings::PoolStore>,
    ) -> Self {
        self.policy = Some(handle);
        self.policy_source = Some(source);
        self.pool = Some(pool);
        self
    }

    /// Refresh the policy handle and advance the cost pool with this tick's measurement.
    ///
    /// A settings change therefore takes effect on the next plan instead of at the next restart,
    /// and the basis follows what the packs actually charge and discharge.
    pub(crate) fn refresh_policy(&self, now: i64, view: &FleetView, meter: &MeterReading) {
        let (Some(handle), Some(source), Some(pool)) =
            (&self.policy, &self.policy_source, &self.pool)
        else {
            return;
        };
        let mut policy = source.policy();
        let stored = pool.load().unwrap_or_default();
        let basis = stored.basis();
        policy.cost_basis_eur_per_kwh = basis;
        handle.set(policy);
        let Some(slot) = self.current_slot(now) else {
            return;
        };
        let hours = self.config.tick_seconds as f64 / 3600.0;
        let (Some(pv), Some(load)) = (meter.pv_kw, meter.load_kw) else {
            return;
        };
        let battery_ac = meter_battery_ac(view) * hours;
        let charge = battery_ac.max(0.0);
        let discharge = (-battery_ac).max(0.0);
        let pv_surplus = (pv - load).max(0.0) * hours;
        let pv_ac = charge.min(pv_surplus);
        let _ = meter;
        let next = crate::savings::advance(
            stored,
            now,
            crate::savings::SlotFlowAc {
                charge_ac: charge,
                pv_ac,
                discharge_ac: discharge,
                buy: slot.buy_all_in,
                feed_in: slot.feed_in,
            },
        );
        if next != stored {
            pool.save(&next);
        }
    }

    /// The plan slot covering `now`, if a plan is loaded.
    fn current_slot(&self, now: i64) -> Option<energy_ledger::plan::PlanSlot> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.plan.as_ref().and_then(|plan| plan.slot_at(now)).cloned())
    }
}

/// The fleet's measured AC power, W: the packs' own reading, never the plan.
fn meter_battery_ac(view: &FleetView) -> f64 {
    view.power_w()
}
