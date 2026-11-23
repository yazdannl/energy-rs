//! Per-slot curtailment: one physical variable priced once by the grid term.

use crate::optimizer::{BindingBound, Slot};

/// The settled balance and costs of one slot at one aggregate battery power.
#[derive(Debug, Clone, PartialEq)]
pub struct Curtailment {
    /// Actually curtailed PV, kW (`0 <= c <= pv`).
    pub curtail_kw: f64,
    /// Settled grid import, kW (negative = export).
    pub grid_kw: f64,
    /// Actual PV surplus after curtailment, kW.
    pub surplus_kw: f64,
    /// Grid cost including the once-priced curtailment loss, EUR for this slot.
    pub grid_cost_eur: f64,
    /// Policy spread floor, EUR-equivalent preference for this slot, never in `total_eur`.
    pub policy_min_spread_eur: f64,
    /// PV policy preference, reported separately and never in `objective_eur`.
    pub policy_preference_eur: f64,
}

fn infeasible_bounds(
    slot_index: usize,
    power_kw: f64,
    g0: f64,
    export_floor: f64,
    import_limit_kw: f64,
) -> Vec<BindingBound> {
    let mut bounds = Vec::new();
    if export_floor.is_finite() && g0 < export_floor {
        bounds.push(BindingBound::new("export_limit", slot_index, -export_floor));
    }
    if g0 > import_limit_kw {
        bounds.push(BindingBound::new(
            "import_limit",
            slot_index,
            import_limit_kw,
        ));
        bounds.push(BindingBound::new("pack_power", slot_index, power_kw.abs()));
    }
    if bounds.is_empty() {
        bounds.push(BindingBound::new("pack_power", slot_index, power_kw.abs()));
    }
    bounds
}

fn preference(slot: &Slot, power_kw: f64, surplus_kw: f64, dt: f64) -> f64 {
    let exportable_surplus = match slot.export_limit_kw {
        None => surplus_kw,
        Some(limit) => surplus_kw.min(limit.max(0.0)),
    };
    (slot.sell_eur_per_kwh - slot.pv_price()) * power_kw.max(0.0).min(exportable_surplus) * dt
}

/// Cost of a settled grid power: buy on import, sell on export.
fn grid_cost(buy: f64, sell: f64, grid_kw: f64, dt: f64) -> f64 {
    dt * (buy * grid_kw.max(0.0) - sell * (-grid_kw).max(0.0))
}

/// Pick the cheapest curtailment point, ties going to the smaller curtailment.
fn cheapest_curtailment(slot: &Slot, dt: f64, c_min: f64, c_max: f64, g0: f64) -> f64 {
    let mut candidates = vec![c_min, c_max];
    let zero_grid = -g0;
    if zero_grid >= c_min && zero_grid <= c_max {
        candidates.push(zero_grid);
    }
    candidates
        .into_iter()
        .min_by(|left, right| {
            grid_cost(slot.buy_eur_per_kwh, slot.sell_eur_per_kwh, g0 + left, dt)
                .total_cmp(&grid_cost(
                    slot.buy_eur_per_kwh,
                    slot.sell_eur_per_kwh,
                    g0 + right,
                    dt,
                ))
                .then_with(|| left.total_cmp(right))
        })
        .unwrap_or(c_min)
}

/// Solve the minimum-cost feasible curtailment for `power_kw`, or return binding bounds.
/// The grid-cost function is piecewise linear with a kink at grid == 0, so its minimum over the
/// feasible interval is one of the two endpoints or that kink.
pub fn solve(
    slot: &Slot,
    slot_index: usize,
    power_kw: f64,
    import_limit_kw: f64,
    min_spread_eur_per_kwh: f64,
) -> Result<Curtailment, Vec<BindingBound>> {
    let dt = slot.dt_h();
    let g0 = slot.load_kw - slot.pv_kw + power_kw;
    let export_floor = slot
        .export_limit_kw
        .map_or(f64::NEG_INFINITY, |limit| -limit);
    let c_min = (export_floor - g0).max(0.0);
    let c_max = (import_limit_kw - g0).min(slot.pv_kw);

    if c_min > c_max + 1e-12 {
        return Err(infeasible_bounds(
            slot_index,
            power_kw,
            g0,
            export_floor,
            import_limit_kw,
        ));
    }

    let c = cheapest_curtailment(slot, dt, c_min, c_max, g0).clamp(0.0, slot.pv_kw);
    let grid_kw = g0 + c;
    let surplus_kw = (slot.pv_kw - c - slot.load_kw).max(0.0);
    let grid_cost_eur = grid_cost(slot.buy_eur_per_kwh, slot.sell_eur_per_kwh, grid_kw, dt);
    let policy_min_spread_eur = dt * min_spread_eur_per_kwh * (power_kw - surplus_kw).max(0.0);
    let policy_preference_eur = preference(slot, power_kw, surplus_kw, dt);

    Ok(Curtailment {
        curtail_kw: c,
        grid_kw,
        surplus_kw,
        grid_cost_eur,
        policy_min_spread_eur,
        policy_preference_eur,
    })
}
