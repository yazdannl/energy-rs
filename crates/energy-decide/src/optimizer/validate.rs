//! Hostile-input barriers: refuse before any DP state is built.

use crate::optimizer::{OptimizerInput, OptimizerRefusal};

/// Validate every finite numeric input, the horizon size and the physics domain.
pub fn validate(input: &OptimizerInput) -> Result<(), OptimizerRefusal> {
    let slots = &input.slots;
    if slots.is_empty() {
        return Err(OptimizerRefusal::EmptyHorizon);
    }
    if slots.len() > 120 {
        return Err(OptimizerRefusal::HorizonSlots { slots: slots.len() });
    }
    validate_config(&input.config)?;
    for (index, slot) in slots.iter().enumerate() {
        validate_slot(slot, index)?;
    }
    Ok(())
}

fn validate_config(config: &crate::optimizer::OptimizerConfig) -> Result<(), OptimizerRefusal> {
    for (field, value) in [
        ("start_kwh", config.start_kwh),
        ("ceiling_kwh", config.ceiling_kwh),
        ("hard_floor_kwh", config.hard_floor_kwh),
        ("soft_guideline_kwh", config.soft_guideline_kwh),
        ("charge_power_w", config.charge_power_w),
        ("discharge_power_w", config.discharge_power_w),
        ("wear", config.wear_eur_per_stored_kwh),
        ("target_penalty", config.target_penalty_eur_per_kwh),
        ("deadband", config.terminal_deadband_kwh),
        ("import_limit_kw", config.import_limit_kw),
        ("min_spread", config.min_spread_eur_per_kwh),
        ("soc_step_kwh", config.soc_step_kwh),
    ] {
        if !value.is_finite() {
            return Err(OptimizerRefusal::NonFiniteConfig { field });
        }
    }
    if !(0.0 < config.eta_charge
        && config.eta_charge <= 1.0
        && 0.0 < config.eta_discharge
        && config.eta_discharge <= 1.0)
    {
        return Err(OptimizerRefusal::InvalidEfficiency {
            eta_charge: config.eta_charge,
            eta_discharge: config.eta_discharge,
        });
    }
    if config.charge_power_w <= 0.0 || config.discharge_power_w <= 0.0 {
        return Err(OptimizerRefusal::InvalidPower {
            charge_w: config.charge_power_w,
            discharge_w: config.discharge_power_w,
        });
    }
    Ok(())
}

fn validate_slot(slot: &crate::optimizer::Slot, index: usize) -> Result<(), OptimizerRefusal> {
    for (field, value) in [
        ("buy", slot.buy_eur_per_kwh),
        ("sell", slot.sell_eur_per_kwh),
        ("load", slot.load_kw),
        ("pv", slot.pv_kw),
    ] {
        if !value.is_finite() {
            return Err(OptimizerRefusal::NonFiniteInput { slot: index, field });
        }
    }
    if let Some(price) = slot.pv_price_eur_per_kwh
        && !price.is_finite()
    {
        return Err(OptimizerRefusal::NonFiniteInput {
            slot: index,
            field: "pv_price",
        });
    }
    if let Some(limit) = slot.export_limit_kw
        && (!limit.is_finite() || limit < 0.0)
    {
        return Err(OptimizerRefusal::NonFiniteInput {
            slot: index,
            field: "export_limit",
        });
    }
    if slot.pv_kw < 0.0 {
        return Err(OptimizerRefusal::InvalidPv {
            slot: index,
            pv_kw: slot.pv_kw,
        });
    }
    if slot.dt_s <= 0 {
        return Err(OptimizerRefusal::InvalidSlotDuration {
            slot: index,
            dt_s: slot.dt_s,
        });
    }
    Ok(())
}
