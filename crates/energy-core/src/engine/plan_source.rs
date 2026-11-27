//! Where a new plan comes from.

use crate::dispatch::BoxFuture;
use crate::dispatch::PackReading;
use crate::plan::{PackState, Plan, PlanError};
use crate::replan::Changes;

/// The plan builder's seam.
///
/// The real implementation is [`LivePlanSource`](crate::plan_source::LivePlanSource), assembled
/// from `energy-ha` statistics and `energy-price` publications; tests return a fixed plan or a
/// typed refusal.
/// `changes` reports upstream refreshes so the replan trigger can invalidate a stored plan without
/// re-reading every input on every tick.
pub trait PlanSource: Send + Sync {
    /// Build a plan for the given packs.
    fn plan(&self, now: i64, packs: Vec<PackState>) -> BoxFuture<'_, Result<Plan, PlanError>>;

    /// Which upstream inputs changed since the last tick.
    fn changes(&self) -> Changes;

    /// One live house-load sample for the windowed forecast bias.
    ///
    /// Sources that do not observe consumption keep the default no-op; the engine passes the
    /// meter's kW reading once per tick, including while it is not replanning.
    fn observe_load(&self, _now: i64, _load_kw: f64) {}

    /// The price-aware full-charge rule, when this source owns one.
    ///
    /// The engine uses the same port for the dwell writer and for manual requests, so a plan and
    /// its balance history can never come from two different policies.
    fn maintenance(&self) -> Option<&crate::maintenance::MaintenancePort> {
        None
    }
}

/// The pack states a plan is built from: every pack with a known SoC.
pub fn plan_states(fleet: &[PackReading]) -> Vec<PackState> {
    fleet.iter().filter_map(PackReading::state).collect()
}
