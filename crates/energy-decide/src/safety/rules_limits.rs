//! Shared physical-limit predicates for safety rules.

use crate::safety::SafetyInput;

pub(crate) fn physical_limits(
    input: &SafetyInput,
    charge: bool,
    required_w: f64,
) -> Vec<&'static str> {
    let mut limits = Vec::new();
    if !required_w.is_finite() {
        limits.push("nonfinite_command");
    }
    if !input.temperature_ok {
        limits.push("temperature_range");
    }
    if !input.current_limit_ok {
        limits.push("capacity_current_limit");
    }
    if charge {
        if !input.max_charge_w.is_finite()
            || input.max_charge_w <= 0.0
            || required_w > input.max_charge_w
        {
            limits.push("max_charge_w");
        }
        if input.at_ceiling() {
            limits.push("soc_ceiling");
        }
    } else if !input.max_discharge_w.is_finite()
        || input.max_discharge_w <= 0.0
        || required_w.abs() > input.max_discharge_w
    {
        limits.push("max_discharge_w");
    }
    if !charge && input.at_floor() {
        limits.push("soc_floor");
    }
    limits
}
