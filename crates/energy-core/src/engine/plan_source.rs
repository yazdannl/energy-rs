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
}

/// The pack states a plan is built from: every pack with a known SoC.
pub fn plan_states(fleet: &[PackReading]) -> Vec<PackState> {
    fleet.iter().filter_map(PackReading::state).collect()
}
