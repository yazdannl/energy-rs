//! The assembled plan: state, evidence, summary and dispatch permission in one typed value.

use crate::horizon::GridSourceCoverage;
use crate::plan_assemble::{
    Acceptance, BindingBound, Delivery, DispatchPermission, PlanState, PlanSummary,
    PlanVsDelivered, TimelineSlot, Unmet,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Generation instant, UTC epoch seconds.
    pub generated_at: i64,
    /// Horizon start and exclusive end, UTC epoch seconds.
    pub horizon_start: i64,
    pub horizon_end: i64,
    /// Requested slots from M1.
    pub requested_slots: usize,
    /// Covered slots from M1.
    pub actual_slots: usize,
    /// Initial state at generation time.
    pub state: PlanState,
    /// Ordered reasons, including every refusal and degradation.
    pub reasons: Vec<&'static str>,
    /// Bounds that bound or clipped the plan.
    pub binding_bounds: Vec<BindingBound>,
    /// Required and optional shortfalls with reason and deadline.
    pub unmet: Vec<Unmet>,
    /// Planned versus delivered rows.
    pub plan_vs_delivered: Vec<PlanVsDelivered>,
    /// Per-source coverage evidence from M1.
    pub grid_source_coverage: Vec<GridSourceCoverage>,
    /// Final timeline, retained even when infeasible.
    pub timeline: Vec<TimelineSlot>,
    /// Plan-owned money and energy totals.
    pub summary: PlanSummary,
    /// Acceptance of a degraded scope, when one was recorded.
    pub acceptance: Option<Acceptance>,
    /// Delivery axis, independent of feasibility.
    pub delivery: Delivery,
    /// Effective terminal target, kWh.
    pub effective_target_kwh: f64,
    /// Raw soft guideline reported separately, kWh.
    pub raw_guideline_kwh: f64,
    /// One-sided terminal shortfall, kWh.
    pub terminal_shortfall_kwh: f64,
    /// Whether optional coverage or obligations are unmet.
    pub degraded: bool,
    /// Scope of the degradation used for acceptance matching.
    pub degraded_scope: &'static str,
}

impl Plan {
    /// State after applying operator-acceptance expiry at an instant.
    pub fn state_at(&self, now: i64) -> PlanState {
        if self.state == PlanState::Infeasible {
            return PlanState::Infeasible;
        }
        if !self.degraded {
            return PlanState::Healthy;
        }
        match &self.acceptance {
            Some(accepted) if now >= accepted.expires_at => PlanState::Infeasible,
            Some(accepted) if !accepted.covers(self.degraded_scope) => PlanState::Infeasible,
            _ => PlanState::DegradedAccepted,
        }
    }

    /// Dispatch permission at an instant.
    pub fn dispatch_permission_at(&self, now: i64) -> DispatchPermission {
        self.state_at(now).dispatch_permission()
    }

    /// Whether an economic setpoint may be taken from this plan at an instant.
    pub fn economic_command_allowed(&self, now: i64) -> bool {
        self.dispatch_permission_at(now) != DispatchPermission::SafetyOnly
    }
}
