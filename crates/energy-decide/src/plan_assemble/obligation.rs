//! A maintenance or terminal outcome with reason, deadline and class.

use crate::plan_assemble::ObligationClass;

#[derive(Debug, Clone, PartialEq)]
pub struct Obligation {
    /// Pack the obligation belongs to.
    pub pack_id: String,
    /// Energy that had to be delivered, kWh.
    pub required_kwh: f64,
    /// Energy the timeline can actually deliver, kWh.
    pub available_kwh: f64,
    /// Typed reason slug, for example horizon_too_short or clamp.
    pub reason: &'static str,
    /// Human-readable deadline, for example 07:00 local.
    pub deadline: String,
    /// Class assigned by the caller; M3 reclassifies using horizon evidence.
    pub class: ObligationClass,
    /// Assignment slot start when one was requested.
    pub t_start: Option<i64>,
    /// Whether the pack is eligible for the work.
    pub eligible: bool,
}
