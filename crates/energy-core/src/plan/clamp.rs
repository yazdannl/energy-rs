//! The reserve and export clamps applied to a solved plan (`docs/POLICY.md` §3 R1, R1b, R4).
//!
//! The solver knows one static SoC band and one price per slot, so two of the policy's bounds cannot
//! be expressed inside it: the per-slot ceiling on *grid* charging, and the ban on exporting a
//! stored kWh below its break-even. Both are applied here, in forward passes over the solver's own
//! action path, and the timeline and the money are recomputed from the result.
//!
//! Nothing here can increase a charge, deepen a discharge, or touch a slot that serves the house.

use energy_opt::BatterySpec;

use crate::plan::{Policy, SLOT_MINUTES};

/// What bound a slot, for the plan's reason string.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // one flag per independent bound
pub(crate) struct Flags {
    /// The grid portion of a charge was cut by the solar ceiling.
    pub ceiling: bool,
    /// The discharge was shortened at the hardware floor.
    pub floor: bool,
    /// A below-break-even battery export was shortened.
    pub gate: bool,
    /// A later charge had to be clipped at the ceiling.
    pub clip: bool,
}

impl Flags {
    /// The reason suffix for a plan row.
    pub const fn reason(self) -> &'static str {
        if self.ceiling {
            "solar_reserve"
        } else if self.clip {
            "pv_clipped"
        } else if self.floor {
            "hardware_floor"
        } else if self.gate {
            "export_below_floor"
        } else {
            ""
        }
    }
}

/// Everything the clamp reads.
pub(crate) struct Request<'a> {
    /// Solver actions, kW (positive charges).
    pub powers: &'a [f64],
    /// Solver SoC at each slot start, kWh.
    pub energies: &'a [f64],
    /// PV forecast, kW.
    pub pv_kw: &'a [f64],
    /// Load forecast, kW.
    pub load_kw: &'a [f64],
    /// Feed-in price per slot, EUR/kWh.
    pub feed_in: &'a [f64],
    /// The aggregate battery.
    pub spec: BatterySpec,
    /// The policy in force.
    pub policy: Policy,
    /// The export gate, when it is armed.
    pub break_even: Option<f64>,
}

/// The clamped path.
pub(crate) struct Outcome {
    /// Clamped actions, kW.
    pub powers: Vec<f64>,
    /// Clamped SoC at each slot start, kWh.
    pub energies: Vec<f64>,
    /// Per-slot bound flags.
    pub flags: Vec<Flags>,
    /// PV a ceiling forced out of the pack, kWh.
    pub clipped_kwh: f64,
    /// Battery energy the export gate kept in the pack, kWh.
    pub gated_kwh: f64,
}

const HOURS: f64 = SLOT_MINUTES / 60.0;

/// Apply every clamp to the solver's path.
pub(crate) fn clamp(request: &Request<'_>) -> Outcome {
    let ceilings = ceilings(request);
    let zero = vec![0.0; request.powers.len()];
    // Pass 1: the clamped path with no capture exemption. Its curtailment is the baseline the budget
    // may spend -- a sale the solver already planned must not hide the PV it makes room for.
    let baseline = pass(request, &ceilings, &zero);
    let rescuable = curtailment(request, &baseline);
    let budget = budget(&rescuable);
    let (powers, energies, flags) = pass(request, &ceilings, &budget);
    let mut clipped_kwh = 0.0;
    let mut gated_kwh = 0.0;
    for (index, flag) in flags.iter().enumerate() {
        let planned = request.powers[index];
        if flag.clip {
            clipped_kwh += (planned - powers[index]).max(0.0) * HOURS;
        }
        if flag.gate {
            gated_kwh += (powers[index] - planned).max(0.0) * HOURS;
        }
    }
    Outcome {
        powers,
        energies,
        flags,
        clipped_kwh,
        gated_kwh,
    }
}

/// The per-slot ceiling on the SoC a *grid* charge may reach (`cap(t)` in the spec).
fn ceilings(request: &Request<'_>) -> Vec<f64> {
    let slots = request.powers.len();
    let mut after = vec![0.0; slots + 1];
    for index in (0..slots).rev() {
        let surplus = (request.pv_kw[index] - request.load_kw[index]).max(0.0) * HOURS;
        after[index] = after[index + 1] + surplus;
    }
    let reserve = request.policy.solar_reserve_kwh(request.spec.e_max_kwh);
    (0..slots)
        .map(|index| {
            (request.spec.e_max_kwh - reserve - after[index + 1])
                .clamp(request.spec.e_min_kwh, request.spec.e_max_kwh)
        })
        .collect()
}

/// Room-limited PV loss per slot on the clamped path: what a sale could actually rescue.
fn curtailment(request: &Request<'_>, baseline: &(Vec<f64>, Vec<f64>, Vec<Flags>)) -> Vec<f64> {
    let energies = &baseline.1;
    let power_limit = request.spec.p_charge_kw * HOURS;
    (0..energies.len())
        .map(|index| {
            let surplus = (request.pv_kw[index] - request.load_kw[index]).max(0.0) * HOURS;
            let room = (request.spec.e_max_kwh - energies[index]).max(0.0) / request.spec.eta_charge;
            let chargeable = surplus.min(room).min(power_limit);
            let curtailed = (surplus - chargeable).max(0.0);
            // Surplus above the charge power is lost whatever the SoC: room cannot rescue it.
            (curtailed - (surplus - power_limit).max(0.0)).max(0.0)
        })
        .collect()
}

/// The backward budget: a slot may spend what the later slots would otherwise lose.
fn budget(rescuable: &[f64]) -> Vec<f64> {
    let mut budget = vec![0.0; rescuable.len()];
    for index in (0..rescuable.len().saturating_sub(1)).rev() {
        budget[index] = budget[index + 1] + rescuable[index + 1];
    }
    budget
}

/// One forward pass with the given capture budget.
fn pass(
    request: &Request<'_>,
    ceilings: &[f64],
    budget: &[f64],
) -> (Vec<f64>, Vec<f64>, Vec<Flags>) {
    let spec = request.spec;
    let slots = request.powers.len();
    let mut powers = Vec::with_capacity(slots);
    let mut energies = Vec::with_capacity(slots);
    let mut flags = Vec::with_capacity(slots);
    let mut energy = request
        .energies
        .first()
        .copied()
        .unwrap_or(spec.e_min_kwh)
        .clamp(spec.e_min_kwh, spec.e_max_kwh);
    for index in 0..slots {
        let start = energy;
        energies.push(start);
        let mut flag = Flags::default();
        let base = request.load_kw[index] - request.pv_kw[index];
        let mut power = bound_charge(
            request, request.powers[index], base, start, ceilings[index], &mut flag,
        );
        if step(spec, start, power) < spec.e_min_kwh {
            power = power.max((spec.e_min_kwh - start) * spec.eta_discharge / HOURS);
            flag.floor = true;
        }
        power = bound_export(request, index, power, base, budget[index], &mut flag);
        energy = step(spec, start, power);
        if energy > spec.e_max_kwh {
            power = (spec.e_max_kwh - start) / (HOURS * spec.eta_charge);
            energy = spec.e_max_kwh;
            flag.clip = true;
        }
        powers.push(power);
        flags.push(flag);
    }
    (powers, energies, flags)
}

/// The grid portion of a charge, bounded by the ceiling; the PV portion always survives.
fn bound_charge(
    request: &Request<'_>,
    power: f64,
    base: f64,
    energy: f64,
    ceiling: f64,
    flag: &mut Flags,
) -> f64 {
    if power <= 0.0 {
        return power;
    }
    let pv_part = power.min((-base).max(0.0));
    let grid_part = power - pv_part;
    let room = (ceiling - energy).max(0.0) / (HOURS * request.spec.eta_charge);
    let bounded = pv_part + grid_part.min(room);
    if bounded < power - 1e-9 {
        flag.ceiling = true;
    }
    bounded
}

/// A discharge that would export battery energy below the break-even, bounded to the house's need
/// plus whatever the capture budget licenses.
fn bound_export(
    request: &Request<'_>,
    index: usize,
    power: f64,
    base: f64,
    budget: f64,
    flag: &mut Flags,
) -> f64 {
    let Some(break_even) = request.break_even else {
        return power;
    };
    if power >= 0.0 || base + power >= 0.0 || request.feed_in[index] >= break_even {
        return power;
    }
    let licensed = budget * request.spec.eta_charge * request.spec.eta_discharge / HOURS;
    let bounded = power.max(-(base.max(0.0) + licensed));
    if bounded > power + 1e-9 {
        flag.gate = true;
    }
    bounded
}

/// Energy after one slot at `power`.
fn step(spec: BatterySpec, energy: f64, power: f64) -> f64 {
    if power >= 0.0 {
        energy + power * HOURS * spec.eta_charge
    } else {
        energy + power * HOURS / spec.eta_discharge
    }
}

#[cfg(test)]
mod tests;
