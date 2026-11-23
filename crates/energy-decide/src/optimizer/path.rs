//! Exact euro cost and canonical signature of a concrete power path.

use crate::optimizer::dp::ActionEval;
use crate::optimizer::feasible;
use crate::optimizer::key::NO_DISCHARGE;
use crate::optimizer::objective::{quantise_wh, terminal_eur, wear_eur};
use crate::optimizer::{OptimizerInput, solve_curtailment};

/// Replay a tail of powers from an exact starting energy; never interpolate or clip.
pub(crate) fn cost(
    input: &OptimizerInput,
    start: f64,
    offset: usize,
    powers: &[i32],
    evals_by_slot: &[Vec<ActionEval>],
    salvage: f64,
) -> Option<(f64, Vec<f64>)> {
    let config = &input.config;
    let target = config.effective_target_kwh();
    let mut energy = start;
    let mut euros = 0.0;
    let mut energies = Vec::with_capacity(powers.len());
    for (step, &power_w) in powers.iter().enumerate() {
        let index = offset + step;
        let next = feasible::next_energy(input, index, energy, power_w, &evals_by_slot[index])?;
        let solved = solve_curtailment(
            &input.slots[index],
            index,
            f64::from(power_w) / 1000.0,
            config.import_limit_kw,
            config.min_spread_eur_per_kwh,
        )
        .ok()?;
        euros += solved.grid_cost_eur + wear_eur(energy, next, config.wear_eur_per_stored_kwh);
        energies.push(next);
        energy = next;
    }
    euros += terminal_eur(
        energy,
        target,
        config.terminal_deadband_kwh,
        config.target_penalty_eur_per_kwh,
        salvage,
    );
    Some((euros, energies))
}

/// Canonical fields of an already replayed path, with p² in W² (no duration factor).
pub(crate) fn fields(
    offset: usize,
    powers: &[i32],
    energies: &[f64],
) -> (usize, i64, usize, Vec<i32>, Vec<i32>) {
    let first = powers
        .iter()
        .position(|power| *power < 0)
        .map_or(NO_DISCHARGE, |position| position + offset);
    let squared: i64 = powers
        .iter()
        .map(|power| i64::from(*power) * i64::from(*power))
        .sum();
    let nonzero = powers.iter().filter(|power| **power != 0).count();
    (
        first,
        squared,
        nonzero,
        energies.iter().copied().map(quantise_wh).collect(),
        powers.to_vec(),
    )
}

#[cfg(test)]
mod tests {
    use super::fields;
    use crate::optimizer::key::{NO_DISCHARGE, path_key};

    #[test]
    fn no_discharge_sentinel_is_stable_at_nonzero_offset() {
        let charging = fields(8, &[1000, 0], &[0.0, 0.0]);
        assert_eq!(charging.0, NO_DISCHARGE);
        let discharging = fields(8, &[0, -1000], &[0.0, 0.0]);
        assert_eq!(discharging.0, 9);

        let no_discharge = path_key(
            0.0,
            charging.0,
            charging.1,
            charging.2,
            &charging.3,
            &charging.4,
        );
        let discharge = path_key(
            0.0,
            discharging.0,
            discharging.1,
            discharging.2,
            &discharging.3,
            &discharging.4,
        );
        assert!(discharge < no_discharge);
    }
}
