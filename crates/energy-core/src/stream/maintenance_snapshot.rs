//! One pack's live maintenance phase.

use energy_battery::protection::full_charge::{FullChargeAssignment, UnassignedPack};

/// The maintenance phase the engine assigned to one pack in the current plan.
#[derive(Debug, Clone, PartialEq)]
pub struct MaintenanceSnapshot {
    /// `charging` before the last cheap slot, `dwell` while balancing at 100 %.
    pub state: &'static str,
    /// The balancing target, 100 %.
    pub target_soc_pct: f64,
    /// End of the last planned charge slot, UTC epoch seconds.
    pub charge_end: i64,
    /// Earliest instant the dwell may complete, UTC epoch seconds.
    pub dwell_until: i64,
    /// Failure cause when the pack is due but could not be scheduled.
    pub unassigned_reason: Option<&'static str>,
    /// AC kWh still missing when the pack is unassigned.
    pub missing_kwh: Option<f64>,
}

impl MaintenanceSnapshot {
    /// A pack with a full charge in the current plan.
    pub fn assigned(entry: &FullChargeAssignment, now: i64) -> Self {
        Self {
            state: if now < entry.charge_end {
                "charging"
            } else {
                "dwell"
            },
            target_soc_pct: entry.target_soc_pct,
            charge_end: entry.charge_end,
            dwell_until: entry.dwell_until,
            unassigned_reason: None,
            missing_kwh: None,
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
