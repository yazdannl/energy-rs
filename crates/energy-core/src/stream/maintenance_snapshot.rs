//! One pack's live maintenance phase.

use energy_battery::protection::full_charge::{FullChargeAssignment, UnassignedPack};

/// The maintenance phase the engine assigned to one pack in the current plan.
#[derive(Debug, Clone, PartialEq)]
pub struct MaintenanceSnapshot {
    /// `charging` before the last cheap slot, `dwell` while balancing at 100 %, or
    /// `charging_partial` for a charge that cannot reach 100 % in this plan.
    pub state: &'static str,
    /// The balancing target, 100 %.
    pub target_soc_pct: f64,
    /// End of the last planned charge slot, UTC epoch seconds.
    pub charge_end: i64,
    /// Earliest instant the dwell may complete, UTC epoch seconds.
    pub dwell_until: i64,
    /// Cause when the pack is due but not fully scheduled (unassigned or partial).
    pub unassigned_reason: Option<&'static str>,
    /// AC kWh still missing when the pack is unassigned or only partially charged.
    pub missing_kwh: Option<f64>,
}

impl MaintenanceSnapshot {
    /// A pack with a full or partial charge in the current plan.
    pub fn assigned(entry: &FullChargeAssignment, now: i64) -> Self {
        let complete = entry.is_complete();
        Self {
            state: if !complete {
                "charging_partial"
            } else if now < entry.charge_end {
                "charging"
            } else {
                "dwell"
            },
            target_soc_pct: entry.target_soc_pct,
            charge_end: entry.charge_end,
            dwell_until: entry.dwell_until,
            unassigned_reason: entry.partial_reason,
            missing_kwh: (!complete).then_some(entry.missing_kwh),
        }
    }

    /// A due pack this plan could not complete, with the machine-readable cause.
    pub fn unassigned(entry: &UnassignedPack) -> Self {
        Self {
            state: match entry.reason {
                "no_headroom" => "unassigned_no_headroom",
                "horizon_too_short" => "unassigned_horizon_too_short",
                "no_cheap_slots" => "unassigned_no_cheap_slots",
                "not_enough_eligible_slots" => "unassigned_not_enough_eligible_slots",
                "soc_ceiling_below_target" => "unassigned_soc_ceiling_below_target",
                _ => "unassigned_insufficient_capacity",
            },
            target_soc_pct: 100.0,
            charge_end: entry.deadline,
            dwell_until: entry.deadline,
            unassigned_reason: Some(entry.reason),
            missing_kwh: Some(entry.missing_kwh),
        }
    }
}
