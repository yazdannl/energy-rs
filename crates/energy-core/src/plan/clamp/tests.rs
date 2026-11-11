//! Unit tests for the reserve and export clamps.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Flags, Request, clamp};
use crate::plan::{ExportPolicy, Policy};

fn spec() -> energy_opt::BatterySpec {
    energy_opt::BatterySpec {
        e_min_kwh: 0.0,
        e_max_kwh: 4.0,
        p_charge_kw: 2.0,
        p_discharge_kw: 2.0,
        eta_charge: 1.0,
        eta_discharge: 1.0,
    }
}

fn policy(solar: f64) -> Policy {
    Policy {
        net_metering: energy_price::settlement::NetMetering::Off,
        export: ExportPolicy::CostBasis,
        solar_reserve_pct: solar,
        peak_reserve_pct: 0.0,
        peak_reserve_value_eur_per_kwh: 0.0,
        wear_eur_per_kwh: 0.0,
        cost_basis_eur_per_kwh: Some(0.10),
        zero_export: false,
    }
}

/// A request with the given arrays; every slot is a quarter-hour.
fn request<'a>(
    powers: &'a [f64],
    energies: &'a [f64],
    pv: &'a [f64],
    load: &'a [f64],
    _feed_in: &'a [f64],
    policy: &'a Policy,
) -> Request<'a> {
    Request {
        powers,
        energies,
        pv_kw: pv,
        load_kw: load,
        spec: spec(),
        policy: *policy,
    }
}

#[test]
fn a_grid_charge_stops_at_the_solar_ceiling_but_the_pv_part_survives() {
    let policy = policy(50.0); // 2 kWh reserve on a 4 kWh pack -> ceiling 2 kWh
    // SoC 1.9, 1 kW PV surplus, a 2 kW charge: the 0.4 kW of grid room is used, the PV kW stays.
    let powers = [2.0, 0.0];
    let energies = [1.9, 2.0];
    let pv = [1.0, 0.0];
    let load = [0.0, 0.0];
    let feed_in = [0.05, 0.05];
    let outcome = clamp(&request(&powers, &energies, &pv, &load, &feed_in, &policy));
    assert!(
        (outcome.powers[0] - 1.4).abs() < 1e-9,
        "{:?}",
        outcome.powers
    );
    assert!(outcome.flags[0].ceiling);
}

#[test]
fn a_below_break_even_export_is_left_to_the_planner() {
    let policy = policy(0.0);
    // Slot 0: the house needs 0.5 kW and the plan discharges 2 kW at a feed-in below the old
    // break-even. Whether that pays is a horizon question (sell now, buy back cheaper), so the clamp
    // does not second-guess it.
    let powers = [-2.0, 0.0];
    let energies = [3.0, 2.5];
    let pv = [0.0, 0.0];
    let load = [0.5, 0.0];
    let feed_in = [0.05, 0.05];
    let outcome = clamp(&request(&powers, &energies, &pv, &load, &feed_in, &policy));
    assert!(
        (outcome.powers[0] + 2.0).abs() < 1e-9,
        "{:?}",
        outcome.powers
    );
}

#[test]
fn a_discharge_never_goes_below_the_hardware_floor() {
    let policy = policy(0.0);
    let powers = [-2.0];
    let energies = [0.4];
    let pv = [0.0];
    let load = [5.0];
    let feed_in = [0.05];
    let outcome = clamp(&request(&powers, &energies, &pv, &load, &feed_in, &policy));
    assert!(outcome.energies[0] + outcome.powers[0] * 0.25 >= -1e-9);
    assert!(outcome.flags[0].floor);
}

#[test]
fn the_gate_is_off_without_a_basis() {
    let mut policy = policy(0.0);
    policy.cost_basis_eur_per_kwh = None;
    let powers = [-2.0];
    let energies = [3.0];
    let pv = [0.0];
    let load = [0.0];
    let feed_in = [0.05];
    let outcome = clamp(&request(&powers, &energies, &pv, &load, &feed_in, &policy));
    assert!((outcome.powers[0] + 2.0).abs() < 1e-9);
    assert_eq!(outcome.flags[0], Flags::default());
}


