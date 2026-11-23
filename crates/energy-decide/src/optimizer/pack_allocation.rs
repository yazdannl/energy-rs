//! Aggregate-to-per-pack allocation with fleet clamp and minimum setpoint in both passes.

use crate::optimizer::{BindingBound, OptimizerConfig};

/// Per-pack setpoints and the clips that produced them.
#[derive(Debug, Clone, PartialEq)]
pub struct PackAllocation {
    /// Signed W per pack, in request order.
    pub powers_w: Vec<i32>,
    /// Named clips (`min_pack_power`, `power_clamp`, `pack_power`), slot 0.
    pub binding_bounds: Vec<BindingBound>,
}

fn clamp_pack(power: i32, charge: i32, discharge: i32, min_w: f64) -> i32 {
    let clipped = power.clamp(-discharge, charge);
    if f64::from(clipped.unsigned_abs()) < min_w {
        0
    } else {
        clipped
    }
}

fn cap_direction(
    powers: &mut [i32],
    cap: i32,
    min_w: f64,
    charge: bool,
    bounds: &mut Vec<BindingBound>,
) {
    let mut remaining = cap;
    let minimum = min_w.ceil() as i32;
    for power in powers.iter_mut() {
        if charge != (*power > 0) || *power == 0 {
            continue;
        }
        let original = *power;
        let granted = original.abs().min(remaining);
        if remaining < minimum || f64::from(granted) < min_w {
            bounds.push(BindingBound::new(
                "power_clamp",
                0,
                f64::from(original) / 1000.0,
            ));
            *power = 0;
            continue;
        }
        *power = if charge { granted } else { -granted };
        remaining -= granted;
        if granted != original.abs() {
            bounds.push(BindingBound::new(
                "power_clamp",
                0,
                f64::from(original.abs() - granted) / 1000.0,
            ));
        }
    }
}

/// Clamp pack physics, then allocate each direction under its own fleet cap. A leftover smaller
/// than `min_pack_power_w` is never emitted: that pack is left at zero and the fleet serves fewer
/// compliant packs.
pub fn allocate_pack_powers(requests_w: &[i32], config: &OptimizerConfig) -> PackAllocation {
    let mut bounds = Vec::new();
    let charge = config.pack_charge_w.round() as i32;
    let discharge = config.pack_discharge_w.round() as i32;
    let min_w = config.min_pack_power_w.max(0.0);
    let mut powers: Vec<i32> = requests_w
        .iter()
        .map(|&request| {
            let clipped = request.clamp(-discharge, charge);
            if clipped != request {
                bounds.push(BindingBound::new(
                    "pack_power",
                    0,
                    f64::from(request) / 1000.0,
                ));
            }
            let compliant = clamp_pack(request, charge, discharge, min_w);
            if compliant == 0 && clipped != 0 {
                bounds.push(BindingBound::new(
                    "min_pack_power",
                    0,
                    f64::from(clipped) / 1000.0,
                ));
            }
            compliant
        })
        .collect();

    cap_direction(
        &mut powers,
        config.charge_power_w.round() as i32,
        min_w,
        true,
        &mut bounds,
    );
    cap_direction(
        &mut powers,
        config.discharge_power_w.round() as i32,
        min_w,
        false,
        &mut bounds,
    );
    PackAllocation {
        powers_w: powers,
        binding_bounds: bounds,
    }
}
