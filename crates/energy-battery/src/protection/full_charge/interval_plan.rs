//! The interval planner's answer: charges and explicit refusals.

use crate::protection::full_charge::{FullChargeAssignment, UnassignedPack};

/// Every due pack is accounted for in exactly one of these lists.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IntervalPlan {
    /// Packs that got a cheap charge before the deadline.
    pub assignments: Vec<FullChargeAssignment>,
    /// Packs that did not, with the reason and the missing energy.
    pub unassigned: Vec<UnassignedPack>,
}
