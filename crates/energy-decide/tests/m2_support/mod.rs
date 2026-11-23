#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::too_many_lines,
    dead_code
)]

//! Deterministic fixtures and an independent objective oracle for the M2 scenarios.

use energy_decide::optimizer::{
    ActionGrid, Curtailment, OptimizerConfig, OptimizerInput, OptimizerRefusal, OptimizerResult,
    ScheduleSlot, Slot, solve, solve_curtailment,
};

/// One slot with unlimited export and no PV/load by default.
pub fn slot(buy: f64, sell: f64, dt_s: i64) -> Slot {
    Slot {
        t_start: 0,
        dt_s,
        buy_eur_per_kwh: buy,
        sell_eur_per_kwh: sell,
        load_kw: 0.0,
        pv_kw: 0.0,
        pv_price_eur_per_kwh: None,
        export_limit_kw: None,
    }
}

/// Live learned-basis configuration.
pub fn live() -> OptimizerConfig {
    OptimizerConfig::default()
}

/// A small, exactly representable configuration for the 1 Wh oracle goldens.
pub fn tiny(start: f64, ceiling: f64, power_w: f64, eta: f64, wear: f64) -> OptimizerConfig {
    OptimizerConfig {
        start_kwh: start,
        ceiling_kwh: ceiling,
        hard_floor_kwh: 0.0,
        soft_guideline_kwh: 0.0,
        charge_power_w: power_w,
        discharge_power_w: power_w,
        eta_charge: eta,
        eta_discharge: eta,
        wear_eur_per_stored_kwh: wear,
        target_penalty_eur_per_kwh: 0.0,
        terminal_deadband_kwh: 0.0,
        ..OptimizerConfig::default()
    }
}

/// Solve a fixture, naming the refusal if one occurs.
pub fn solved(input: &OptimizerInput) -> OptimizerResult {
    solve(input).unwrap_or_else(|error| panic!("fixture must solve: {error:?}"))
}

/// Every accepted slot conserves energy and obeys power, curtailment and site bounds.
pub fn assert_physical(input: &OptimizerInput, result: &OptimizerResult) {
    let config = &input.config;
    for row in &result.schedule {
        let slot = &input.slots[row.slot];
        let dt = slot.dt_h();
        let power_kw = f64::from(row.power_w) / 1000.0;
        let expected = dt
            * power_kw
            * if power_kw >= 0.0 {
                config.eta_charge
            } else {
                1.0 / config.eta_discharge
            };
        assert!(
            (row.energy_end_kwh - row.energy_start_kwh - expected).abs() <= 1e-9,
            "slot {} energy conservation",
            row.slot
        );
        assert!(row.energy_end_kwh >= config.hard_floor_kwh - 1e-9);
        assert!(row.energy_end_kwh <= config.ceiling_kwh + 1e-9);
        assert!(row.power_w <= config.charge_power_w.round() as i32);
        assert!(row.power_w >= -(config.discharge_power_w.round() as i32));
        assert!(row.curtail_kw >= 0.0 && row.curtail_kw <= slot.pv_kw + 1e-9);
        let grid = slot.load_kw - slot.pv_kw + row.curtail_kw + power_kw;
        assert!((grid - row.grid_kw).abs() <= 1e-9);
        assert!(grid <= config.import_limit_kw + 1e-9);
        if let Some(limit) = slot.export_limit_kw {
            assert!(grid >= -limit - 1e-9);
        }
    }
    assert_eq!(result.schedule.len(), input.slots.len());
    let terminal = result
        .schedule
        .last()
        .map_or(config.start_kwh, |row| row.energy_end_kwh);
    assert!((result.terminal_kwh - terminal).abs() <= 1e-9);
}

/// Exhaustive 1 Wh / 1 W oracle for horizons of at most three slots.
pub fn brute_force(input: &OptimizerInput) -> (f64, Vec<i32>) {
    let actions = ActionGrid::new(
        input.config.charge_power_w,
        input.config.discharge_power_w,
        input.config.power_steps,
    );
    let mut search = Search {
        input,
        actions: &actions.powers_w,
        path: Vec::new(),
        best: None,
    };
    enumerate(&mut search, 0, input.config.start_kwh, 0.0);
    search
        .best
        .expect("at least the zero-power path is feasible")
}

struct Search<'a> {
    input: &'a OptimizerInput,
    actions: &'a [i32],
    path: Vec<i32>,
    best: Option<(f64, Vec<i32>)>,
}

fn enumerate(search: &mut Search<'_>, index: usize, energy: f64, cost: f64) {
    let input = search.input;
    if index == input.slots.len() {
        let config = &input.config;
        let target = config.effective_target_kwh();
        let cheapest = input
            .slots
            .iter()
            .map(|slot| slot.buy_eur_per_kwh)
            .fold(f64::INFINITY, f64::min);
        let total = cost - cheapest / config.eta_charge * energy
            + config.target_penalty_eur_per_kwh
                * (target - energy - config.terminal_deadband_kwh).max(0.0);
        if search
            .best
            .as_ref()
            .is_none_or(|(previous, _)| total < *previous)
        {
            search.best = Some((total, search.path.clone()));
        }
        return;
    }
    let slot = &input.slots[index];
    let config = &input.config;
    for &power_w in search.actions {
        let power_kw = f64::from(power_w) / 1000.0;
        let next = energy
            + slot.dt_h()
                * power_kw
                * if power_kw >= 0.0 {
                    config.eta_charge
                } else {
                    1.0 / config.eta_discharge
                };
        if next < config.hard_floor_kwh - 1e-9 || next > config.ceiling_kwh + 1e-9 {
            continue;
        }
        let Ok(solved) = solve_curtailment(
            slot,
            index,
            power_kw,
            config.import_limit_kw,
            config.min_spread_eur_per_kwh,
        ) else {
            continue;
        };
        let wear = config.wear_eur_per_stored_kwh * (energy - next).max(0.0);
        search.path.push(power_w);
        enumerate(search, index + 1, next, cost + solved.grid_cost_eur + wear);
        search.path.pop();
    }
}

/// Independent backward-DP objective oracle over a configurable energy grid.
///
/// The 1 Wh oracle is used for the small goldens; the 120-slot fixture uses 20 Wh so the test
/// suite stays runnable on this 10 GiB box. Both sweep the same canonical power grid.
pub fn dp_objective(input: &OptimizerInput, step_kwh: f64) -> f64 {
    let config = &input.config;
    let points = grid(
        config.hard_floor_kwh,
        config.ceiling_kwh,
        step_kwh,
        config.start_kwh,
    );
    let count = input.slots.len();
    let mut values = vec![vec![0.0; points.len()]; count + 1];
    let target = config.effective_target_kwh();
    let salvage = input
        .slots
        .iter()
        .map(|slot| slot.buy_eur_per_kwh)
        .fold(f64::INFINITY, f64::min)
        / config.eta_charge;
    for (cell, point) in points.iter().enumerate() {
        values[count][cell] = -salvage * point
            + config.target_penalty_eur_per_kwh
                * (target - point - config.terminal_deadband_kwh).max(0.0);
    }
    let actions = ActionGrid::new(
        config.charge_power_w,
        config.discharge_power_w,
        config.power_steps,
    );
    for index in (0..count).rev() {
        for (cell, point) in points.iter().enumerate() {
            values[index][cell] = slot_best(input, &actions, &points, &values, index, *point);
        }
    }
    interpolate(&points, &values[0], config.start_kwh)
}

fn slot_best(
    input: &OptimizerInput,
    actions: &ActionGrid,
    points: &[f64],
    values: &[Vec<f64>],
    index: usize,
    point: f64,
) -> f64 {
    let config = &input.config;
    let slot = &input.slots[index];
    let mut best = f64::INFINITY;
    for &power_w in &actions.powers_w {
        let power_kw = f64::from(power_w) / 1000.0;
        let next = point
            + slot.dt_h()
                * power_kw
                * if power_kw >= 0.0 {
                    config.eta_charge
                } else {
                    1.0 / config.eta_discharge
                };
        if next < config.hard_floor_kwh - 1e-9 || next > config.ceiling_kwh + 1e-9 {
            continue;
        }
        let Ok(solved) = solve_curtailment(
            slot,
            index,
            power_kw,
            config.import_limit_kw,
            config.min_spread_eur_per_kwh,
        ) else {
            continue;
        };
        let wear = config.wear_eur_per_stored_kwh * (point - next).max(0.0);
        let candidate = solved.grid_cost_eur + wear + interpolate(points, &values[index + 1], next);
        if candidate < best {
            best = candidate;
        }
    }
    best
}

fn grid(floor: f64, ceiling: f64, step: f64, anchor: f64) -> Vec<f64> {
    let mut points: Vec<f64> = Vec::new();
    let mut value = floor;
    while value <= ceiling {
        points.push(value);
        value += step;
    }
    if anchor >= floor && anchor <= ceiling {
        points.push(anchor);
    }
    points.sort_by(f64::total_cmp);
    points.dedup_by(|left, right| (*left - *right).abs() <= 1e-9);
    points
}

fn interpolate(points: &[f64], values: &[f64], energy: f64) -> f64 {
    if energy <= points[0] {
        return values[0];
    }
    let upper = points.partition_point(|point| *point <= energy);
    if upper >= points.len() {
        return values[values.len() - 1];
    }
    let lower = upper - 1;
    let span = points[upper] - points[lower];
    if span <= 0.0 {
        return values[lower];
    }
    values[lower] + (energy - points[lower]) / span * (values[upper] - values[lower])
}

/// The verified 120-slot fixture from SPEC/S164 (prices EUR/kWh, zero net load).
pub fn fixture_prices() -> Vec<f64> {
    let today = [
        0.3980, 0.4037, 0.4044, 0.4072, 0.4111, 0.4130, 0.4064, 0.4074, 0.4061, 0.4188, 0.4084,
        0.4029, 0.3969,
    ];
    let tomorrow = [
        0.4790, 0.4940, 0.5078, 0.5099, 0.4968, 0.5098, 0.5114, 0.5303, 0.5188, 0.5156, 0.5125,
        0.5059, 0.5108, 0.4967, 0.4899, 0.4861,
    ];
    (0..120)
        .map(|index| {
            if index < today.len() {
                today[index]
            } else if (93..93 + tomorrow.len()).contains(&index) {
                tomorrow[index - 93]
            } else {
                0.3498
            }
        })
        .collect()
}

/// Expected refusal reason helper.
pub fn refused(input: &OptimizerInput) -> OptimizerRefusal {
    solve(input).expect_err("fixture must refuse")
}

/// Direct curtailment evaluation for the S70/S117/S120 algebra tests.
pub fn curtail(
    input: &OptimizerInput,
    index: usize,
    power_kw: f64,
) -> Result<Curtailment, Vec<energy_decide::optimizer::BindingBound>> {
    solve_curtailment(
        &input.slots[index],
        index,
        power_kw,
        input.config.import_limit_kw,
        input.config.min_spread_eur_per_kwh,
    )
}

/// One solved row's exact grid power (kept for readable assertions).
pub fn grid_power(row: &ScheduleSlot) -> f64 {
    row.grid_kw
}
