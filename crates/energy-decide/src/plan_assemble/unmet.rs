//! Every shortfall record, required or optional, carries reason, deadline and class.

use crate::plan_assemble::ObligationClass;

#[derive(Debug, Clone, PartialEq)]
pub struct Unmet {
    /// Obligation name, for example terminal_target or maintenance.
    pub obligation: String,
    /// Affected pack.
    pub pack_id: String,
    /// Requested energy, kWh.
    pub required_kwh: f64,
    /// Deliverable energy, kWh.
    pub available_kwh: f64,
    /// Requested minus available, kWh.
    pub shortfall_kwh: f64,
    /// Typed reason slug.
    pub reason: &'static str,
    /// Human-readable deadline.
    pub deadline: String,
    /// Required or optional class.
    pub class: ObligationClass,
    /// Assignment slot start, when one was requested.
    pub t_start: Option<i64>,
    /// Separate marker for a missing assignment slot, preserving the named unassigned reason.
    pub assignment_slot_missing: bool,
}
