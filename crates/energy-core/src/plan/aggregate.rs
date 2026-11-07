//! Collapse the fleet into the one battery the optimizer models.

use energy_opt::BatterySpec;

use crate::plan::PlanInput;

/// The aggregate battery and the scale the per-pack split uses later.
pub(crate) struct Aggregate {
    /// The six physical values the DP reads.
    pub spec: BatterySpec,
    /// Stored energy at plan time, kWh.
    pub start_kwh: f64,
    /// Summed configured capacity, kWh.
    pub capacity_kwh: f64,
    /// Capacity-weighted charge efficiency.
    pub eta_charge: f64,
    /// Capacity-weighted discharge efficiency.
    pub eta_discharge: f64,
}

/// The fleet seen as one battery; capacity-weighted efficiencies, never an unweighted mean.
pub(crate) fn aggregate(input: &PlanInput) -> Aggregate {
    let capacity: f64 = input.packs.iter().map(|p| p.spec.capacity_kwh).sum();
    let start_kwh = input
        .packs
        .iter()
        .map(|p| p.spec.energy_kwh(p.soc_pct))
        .sum();
    let e_min_kwh = input
        .packs
        .iter()
        .map(|p| p.spec.e_min_kwh(input.reserve_soc_pct))
        .sum();
    let e_max_kwh = input.packs.iter().map(|p| p.spec.e_max_kwh()).sum();
    let p_charge_kw = input.packs.iter().map(|p| p.spec.max_charge_w).sum::<f64>() / 1000.0;
    let p_discharge_kw = input
        .packs
        .iter()
        .map(|p| p.spec.max_discharge_w)
        .sum::<f64>()
        / 1000.0;
    let eta_charge = weighted_eta(input, true);
    let eta_discharge = weighted_eta(input, false);
    Aggregate {
        spec: BatterySpec {
            e_min_kwh,
            e_max_kwh,
            p_charge_kw,
            p_discharge_kw,
            eta_charge,
            eta_discharge,
        },
        start_kwh,
        capacity_kwh: capacity,
        eta_charge,
        eta_discharge,
    }
}

fn weighted_eta(input: &PlanInput, charging: bool) -> f64 {
    let capacity: f64 = input.packs.iter().map(|p| p.spec.capacity_kwh).sum();
    if capacity <= 0.0 {
        return 1.0;
    }
    let weighted: f64 = input
        .packs
        .iter()
        .map(|p| {
            let eta = if charging {
                p.spec.eta_charge
            } else {
                p.spec.eta_discharge
            };
            p.spec.capacity_kwh * eta
        })
        .sum();
    weighted / capacity
}
