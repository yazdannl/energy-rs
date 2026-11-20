//! A due pack that could not be scheduled in this plan, and the reason why.

use crate::protection::full_charge::{ChargeCandidate, DuePack, IntervalPlan};

/// Tolerance for treating a pack's missing energy as zero.
const ENERGY_EPS_KWH: f64 = 1e-6;

/// A due pack that could not be scheduled in this plan.
#[derive(Debug, Clone, PartialEq)]
pub struct UnassignedPack {
    /// Pack serial.
    pub pack_id: String,
    /// `horizon_too_short`, `no_cheap_slots`, `not_enough_eligible_slots`, `no_headroom`,
    /// `insufficient_capacity` or `soc_ceiling_below_target`.
    pub reason: &'static str,
    /// AC kWh needed to reach 100 %.
    pub required_kwh: f64,
    /// AC kWh this plan could have delivered inside the cheap band.
    pub available_kwh: f64,
    /// AC kWh still missing.
    pub missing_kwh: f64,
    /// Local 07:00 completion deadline, UTC epoch seconds.
    pub deadline: i64,
}

/// Every due pack gets an explicit refusal when nothing can be planned at all.
pub(crate) fn unassigned_all(
    due: Vec<&DuePack>,
    deadline: i64,
    window_slots: usize,
    reason: &'static str,
) -> IntervalPlan {
    IntervalPlan {
        assignments: Vec::new(),
        unassigned: due
            .into_iter()
            .map(|pack| UnassignedPack {
                pack_id: pack.pack_id.clone(),
                reason: if window_slots == 0 {
                    "horizon_too_short"
                } else {
                    reason
                },
                required_kwh: pack.ac_energy_needed_kwh(),
                available_kwh: 0.0,
                missing_kwh: pack.ac_energy_needed_kwh(),
                deadline,
            })
            .collect(),
    }
}

/// A pack whose configured SoC ceiling is below the balancing target cannot be completed.
pub(crate) fn ceiling_refusal(pack: &DuePack, deadline: i64) -> UnassignedPack {
    let required = pack.ac_energy_needed_kwh();
    UnassignedPack {
        pack_id: pack.pack_id.clone(),
        reason: "soc_ceiling_below_target",
        required_kwh: required,
        available_kwh: 0.0,
        missing_kwh: required,
        deadline,
    }
}

/// Why this pack was left out, with the energy still missing.
pub(crate) fn shortfall(
    pack: &DuePack,
    eligible: &[ChargeCandidate],
    remaining: &[f64],
    priced_without_headroom: bool,
    deadline: i64,
) -> UnassignedPack {
    let required = pack.ac_energy_needed_kwh();
    let available: f64 = eligible
        .iter()
        .zip(remaining.iter())
        .map(|(slot, watts)| {
            let seconds = (slot.t_end - slot.t_start).max(0) as f64;
            watts.max(0.0).min(pack.max_charge_w) * seconds / 3_600_000.0
        })
        .sum();
    // Physical energy of the eligible window before other packs consume its headroom.
    let window_kwh: f64 = eligible
        .iter()
        .map(|slot| pack.max_charge_w * (slot.t_end - slot.t_start).max(0) as f64 / 3_600_000.0)
        .sum();
    let reason = if eligible.is_empty() && priced_without_headroom {
        "no_headroom"
    } else if eligible.is_empty() {
        "no_cheap_slots"
    } else if window_kwh + ENERGY_EPS_KWH < required {
        "not_enough_eligible_slots"
    } else if available <= ENERGY_EPS_KWH {
        "no_headroom"
    } else {
        "insufficient_capacity"
    };
    UnassignedPack {
        pack_id: pack.pack_id.clone(),
        reason,
        required_kwh: required,
        available_kwh: available,
        missing_kwh: (required - available).max(0.0),
        deadline,
    }
}
