//! The aligned action set: charge exactly the PV surplus, discharge exactly the house.
//!
//! A 24-step lattice cannot express either, so it rounds: a 0.88 kW load lands on 1.0087 kW and
//! spills 0.13 kW to the grid, and a 0.10 kW surplus either goes to the grid or drags 0.14 kW of
//! import with it. These tests pin both candidates and their effect on the chosen path.

// Test code: a failed assertion is the report.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use energy_opt::{BatterySpec, Config, SolveInput, solve};

fn config(align: bool) -> Config {
    let spec = BatterySpec {
        e_min_kwh: 0.8,
        e_max_kwh: 8.0,
        p_charge_kw: 4.0,
        p_discharge_kw: 3.2,
        eta_charge: 0.95,
        eta_discharge: 0.95,
    };
    let mut config = Config::new(spec, 4.0);
    config.slot_minutes = 15.0;
    config.soc_step_kwh = 0.05;
    config.power_steps = 24;
    config.max_states = 400;
    config.max_iterations = 2_000_000;
    config.allow_grid_charge = true;
    config.allow_export = true;
    config.wear_eur_per_kwh = 0.0;
    config.salvage = 0.20;
    config.align_actions = align;
    config
}

fn solve_one(load_kw: f64, pv_kw: f64, buy: f64, sell: f64, align: bool) -> f64 {
    let minutes = [15.0];
    let pv = [pv_kw];
    let load = [load_kw];
    let prices = [buy];
    let feed_in = [sell];
    let input = SolveInput::new(&pv, &load, &prices, &feed_in, &minutes);
    let result = solve(&input, &config(align)).expect("solve");
    result.powers[0]
}

#[test]
fn serving_the_house_exactly_beats_spilling_the_lattice_remainder() {
    // Without the candidate the nearest actions are 0.6957 and 1.0087 kW; the deeper one spills.
    let unaligned = solve_one(0.88, 0.0, 0.50, 0.10, false);
    assert!(
        unaligned < -0.95,
        "the unaligned lattice should overshoot the load, got {unaligned}"
    );
    let aligned = solve_one(0.88, 0.0, 0.50, 0.10, true);
    assert!(
        (aligned + 0.88).abs() < 1e-9,
        "the aligned plan should serve the load exactly, got {aligned}"
    );
}

#[test]
fn charging_exactly_the_surplus_beats_importing_the_lattice_remainder() {
    // A 0.10 kW surplus with the smallest lattice charge at 0.2435 kW: the charge drags 0.14 kW of
    // import at a buy price above what the stored energy is worth, so the unaligned plan just
    // exports the surplus.
    let unaligned = solve_one(0.30, 0.40, 0.30, 0.05, false);
    assert!(
        unaligned.abs() < 1e-9,
        "the unaligned plan should not charge, got {unaligned}"
    );
    let aligned = solve_one(0.30, 0.40, 0.30, 0.05, true);
    assert!(
        (aligned - 0.10).abs() < 1e-9,
        "the aligned plan should charge exactly the surplus, got {aligned}"
    );
}
