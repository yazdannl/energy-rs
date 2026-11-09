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
    feed_in: &'a [f64],
    policy: &'a Policy,
) -> Request<'a> {
    Request {
        powers,
        energies,
        pv_kw: pv,
        load_kw: load,
        feed_in,
        spec: spec(),
        policy: *policy,
        break_even: policy.break_even(1.0),
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
fn a_below_break_even_export_is_shortened_to_the_house_need() {
    let policy = policy(0.0);
    // Slot 0: house needs 0.5 kW, the plan discharges 2 kW at a feed-in below break-even.
    let powers = [-2.0, 0.0];
    let energies = [3.0, 2.5];
    let pv = [0.0, 0.0];
    let load = [0.5, 0.0];
    let feed_in = [0.05, 0.05];
    let outcome = clamp(&request(&powers, &energies, &pv, &load, &feed_in, &policy));
    assert!(
        (outcome.powers[0] + 0.5).abs() < 1e-9,
        "{:?}",
        outcome.powers
    );
    assert!(outcome.flags[0].gate);
    assert!(outcome.gated_kwh > 0.0);
}

#[test]
fn an_export_above_the_break_even_is_left_alone() {
    let policy = policy(0.0);
    let powers = [-2.0];
    let energies = [3.0];
    let pv = [0.0];
    let load = [0.0];
    let feed_in = [0.30];
    let outcome = clamp(&request(&powers, &energies, &pv, &load, &feed_in, &policy));
    assert!((outcome.powers[0] + 2.0).abs() < 1e-9);
    assert!(!outcome.flags[0].gate);
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

#[test]
fn the_capture_budget_licenses_a_sale_only_when_pv_would_be_lost() {
    let policy = policy(50.0); // ceiling 2 kWh, so a full pack can hold no more PV
    // Full pack, then a 3 kW surplus that has nowhere to go, then a second sale slot.
    let powers = [-2.0, -2.0, 0.0];
    let energies = [4.0, 3.5, 3.0];
    let pv = [0.0, 3.0, 0.0];
    let load = [0.0, 0.0, 0.0];
    let feed_in = [0.05, 0.05, 0.05];
    let outcome = clamp(&request(&powers, &energies, &pv, &load, &feed_in, &policy));
    // The budget is the room-limited surplus (0.5 kWh, the 2 kW charge power), which licenses
    // 0.5 kWh of sale: the first slot sells, the second is gated.
    assert!(
        (outcome.powers[0] + 2.0).abs() < 1e-9,
        "{:?}",
        outcome.powers
    );
    assert!(outcome.powers[1].abs() < 1e-9, "{:?}", outcome.powers);
    assert!(outcome.flags[1].gate);
}

#[test]
fn no_surplus_ahead_means_no_exemption() {
    let policy = policy(50.0);
    let powers = [-2.0, 0.0];
    let energies = [4.0, 3.5];
    let pv = [0.0, 0.0];
    let load = [0.0, 0.0];
    let feed_in = [0.05, 0.05];
    let outcome = clamp(&request(&powers, &energies, &pv, &load, &feed_in, &policy));
    assert!((outcome.powers[0]).abs() < 1e-9, "{:?}", outcome.powers);
}
