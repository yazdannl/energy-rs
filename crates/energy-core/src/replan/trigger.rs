//! The pure trigger evaluation; the engine turns a reason into a hold or a solve.

use crate::plan::{Plan, SLOT_SECONDS};
use crate::replan::{
    Changes, LiveState, REPLAN_SECONDS, ReplanReason, SOC_DEVIATION_PCT, STALE_PLAN_SECONDS,
};

/// Replan policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplanTrigger {
    /// Normal replan interval, seconds.
    pub replan_seconds: i64,
    /// Absolute stale-plan bound, seconds.
    pub stale_plan_seconds: i64,
    /// SoC deviation threshold, percent.
    pub soc_deviation_pct: f64,
    /// Clock slot length, seconds.
    pub slot_seconds: i64,
}

impl Default for ReplanTrigger {
    fn default() -> Self {
        Self {
            replan_seconds: REPLAN_SECONDS,
            stale_plan_seconds: STALE_PLAN_SECONDS,
            soc_deviation_pct: SOC_DEVIATION_PCT,
            slot_seconds: SLOT_SECONDS,
        }
    }
}

impl ReplanTrigger {
    /// Why the plan must be replaced, or `None` while it is still valid.
    ///
    /// Staleness wins over a crossed boundary: an old plan is not kept just because a slot happens
    /// to have rolled over (survey §7 trap 4).
    pub fn reason(
        &self,
        now: i64,
        plan: Option<&Plan>,
        live: &LiveState,
        changes: &Changes,
    ) -> Option<ReplanReason> {
        let Some(plan) = plan else {
            return Some(ReplanReason::NoPlan);
        };
        let stale_after = self
            .replan_seconds
            .saturating_mul(2)
            .max(self.stale_plan_seconds);
        if now.saturating_sub(plan.generated_at) > stale_after {
            return Some(ReplanReason::StaleInput);
        }
        if now >= plan.horizon_end {
            return Some(ReplanReason::PlanExpired);
        }
        // The clock the plan runs on: `replan_seconds` is a refresh cadence, not only a staleness
        // bound. Without this a quiet day would keep one plan until the next slot boundary.
        if self.replan_seconds > 0 && now.saturating_sub(plan.generated_at) >= self.replan_seconds {
            return Some(ReplanReason::Due);
        }
        if changes.prices {
            return Some(ReplanReason::PriceChanged);
        }
        if changes.forecast {
            return Some(ReplanReason::ForecastChanged);
        }
        if self.soc_deviated(now, plan, live) {
            return Some(ReplanReason::SocDeviation);
        }
        if now.div_euclid(self.slot_seconds) > plan.generated_at.div_euclid(self.slot_seconds) {
            return Some(ReplanReason::SlotBoundary);
        }
        None
    }

    fn soc_deviated(&self, now: i64, plan: &Plan, live: &LiveState) -> bool {
        let Some(slot) = plan.slot_at(now) else {
            return false;
        };
        if live.capacity_kwh <= 0.0 || !live.energy_kwh.is_finite() {
            return false;
        }
        let deviation = (live.energy_kwh - slot.soc_kwh).abs() / live.capacity_kwh * 100.0;
        deviation >= self.soc_deviation_pct
    }
}
