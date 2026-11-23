//! Everything M3 needs to validate an already-solved timeline.

use crate::horizon::{Horizon, HorizonRefusal};
use crate::plan_assemble::{Acceptance, Bounds, Delivery, Obligation, Targets, TimelineSlot};

#[derive(Debug, Clone, PartialEq)]
pub struct PlanInput {
    /// Plan generation instant, UTC epoch seconds.
    pub generated_at: i64,
    /// Typed M1 horizon, including refusals and coverage.
    pub horizon: Horizon,
    /// Solved and clamped timeline, before maintenance was applied.
    pub slots: Vec<TimelineSlot>,
    /// Hard bounds validated before solving.
    pub bounds: Bounds,
    /// Terminal target inputs.
    pub targets: Targets,
    /// Required and optional outcomes, including maintenance assignments.
    pub obligations: Vec<Obligation>,
    /// A typed M1 refusal, when no usable horizon was emitted.
    pub refusal: Option<HorizonRefusal>,
    /// Optional operator acceptance of degraded coverage.
    pub acceptance: Option<Acceptance>,
    /// Previously classified delivery evidence.
    pub delivery: Delivery,
}
