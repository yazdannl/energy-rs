//! Shape, continuity, physical balance and power checks on the final timeline.

use crate::plan_assemble::{BindingBound, PlanInput, TimelineSlot, Validation};

fn bind(name: &'static str, t_start: Option<i64>, value: f64) -> BindingBound {
    BindingBound {
        reason: name,
        slot: t_start,
        kw_or_kwh: value,
    }
}

pub fn check_shape(input: &PlanInput, result: &mut Validation) {
    let expected = input.horizon.actual_slots();
    if input.slots.len() != expected {
        result.binding(bind("timeline_length", None, expected as f64));
        result.hard("timeline_length");
    }
    let mut previous_end = None;
    for (index, slot) in input.slots.iter().enumerate() {
        let wanted = input
            .horizon
            .t0
            .saturating_add(i64::try_from(index).unwrap_or(i64::MAX) * input.bounds.slot_seconds);
        if slot.t_start != wanted {
            result.binding(bind("timeline_order", Some(slot.t_start), wanted as f64));
            result.hard("timeline_order");
        }
        if previous_end.is_some_and(|end: f64| (slot.e_start_kwh - end).abs() > 1e-9) {
            result.binding(bind(
                "timeline_discontinuity",
                Some(slot.t_start),
                slot.e_start_kwh,
            ));
            result.hard("timeline_discontinuity");
        }
        previous_end = Some(slot.e_end_kwh);
    }
}

pub fn slot_is_finite(slot: &TimelineSlot) -> bool {
    slot.e_start_kwh.is_finite()
        && slot.e_end_kwh.is_finite()
        && slot.grid_w.is_finite()
        && slot.pv_w.is_finite()
        && slot.load_w.is_finite()
        && slot.battery_w.is_finite()
        && slot.pv_headroom_kwh.is_finite()
        && slot
            .pack_powers
            .iter()
            .all(|pack| pack.w.is_finite() && pack.pack_id.len() <= 128)
}

pub fn check_slot(
    slot: &TimelineSlot,
    bounds: &crate::plan_assemble::Bounds,
    within_terminal_tolerance: bool,
    result: &mut Validation,
) {
    if slot.e_end_kwh < bounds.floor_kwh && !within_terminal_tolerance {
        result.binding(bind("soc_floor", Some(slot.t_start), bounds.floor_kwh));
        result.hard("soc_floor");
    }
    if slot.e_end_kwh > bounds.ceiling_kwh {
        result.binding(bind("soc_ceiling", Some(slot.t_start), bounds.ceiling_kwh));
        result.hard("soc_ceiling");
    }
    let hours = bounds.slot_seconds as f64 / 3600.0;
    let expected = expected_energy(slot.e_start_kwh, slot.battery_w, bounds, hours);
    if (expected - slot.e_end_kwh).abs() > 0.001 {
        result.binding(bind("energy_conservation", Some(slot.t_start), expected));
        result.hard("energy_conservation");
    }
    check_powers(slot, bounds, result);
    if slot.grid_w > bounds.import_ceiling_w {
        result.binding(bind("import_ceiling", Some(slot.t_start), slot.grid_w));
        result.hard("import_ceiling");
    }
    if slot.grid_w < -bounds.export_ceiling_w {
        result.binding(bind(
            "export_ceiling",
            Some(slot.t_start),
            slot.grid_w.abs(),
        ));
        result.hard("export_ceiling");
    }
    if !slot.cell_ok || !slot.thermal_ok {
        let reason = if slot.cell_ok {
            "thermal_limit"
        } else {
            "cell_voltage_guard"
        };
        result.binding(bind(reason, Some(slot.t_start), 0.0));
        result.hard(reason);
    }
}

pub fn expected_energy(
    start: f64,
    battery_w: f64,
    bounds: &crate::plan_assemble::Bounds,
    hours: f64,
) -> f64 {
    if battery_w >= 0.0 {
        start + battery_w * hours / 1000.0 * bounds.eta_charge
    } else {
        start + battery_w * hours / 1000.0 / bounds.eta_discharge
    }
}

fn check_powers(
    slot: &TimelineSlot,
    bounds: &crate::plan_assemble::Bounds,
    result: &mut Validation,
) {
    let mut charge = 0.0;
    let mut discharge = 0.0;
    for pack in &slot.pack_powers {
        charge += pack.w.max(0.0);
        discharge += (-pack.w).max(0.0);
        if pack.w > bounds.charge_limit_w || -pack.w > bounds.discharge_limit_w {
            result.binding(bind(
                "pack_power",
                Some(slot.t_start),
                pack.w.abs() / 1000.0,
            ));
            result.hard("pack_power");
        }
    }
    let aggregate_charge = slot.battery_w.max(0.0);
    let aggregate_discharge = (-slot.battery_w).max(0.0);
    if (charge - aggregate_charge).abs() > 1.0 || (discharge - aggregate_discharge).abs() > 1.0 {
        result.binding(bind(
            "pack_battery_balance",
            Some(slot.t_start),
            charge / 1000.0,
        ));
        result.hard("pack_battery_balance");
    }
    if aggregate_charge > bounds.charge_limit_w || aggregate_discharge > bounds.discharge_limit_w {
        result.binding(bind(
            "power_clamp",
            Some(slot.t_start),
            slot.battery_w.abs() / 1000.0,
        ));
        result.hard("power_clamp");
    }
    let expected_grid = slot.load_w - slot.pv_w + slot.battery_w;
    if (expected_grid - slot.grid_w).abs() > 1.0 {
        result.binding(bind("grid_balance", Some(slot.t_start), expected_grid));
        result.hard("grid_balance");
    }
}
