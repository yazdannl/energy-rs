//! Differential test: the C core against the Rust brute-force oracle.
//!
//! [`energy_opt::oracle`] is a second, deliberately naive implementation of the same DP.  It shares
//! no arithmetic with the C core and no helper with the validator, so a disagreement here is a
//! real disagreement.  Over the parity fixtures this test compares
//!
//! * the whole value function, the state grid, the objective and the reconstructed path at 1e-9;
//! * the iteration and state counts exactly;
//! * **every** stored decision (`argmin[i][k]`, the full `slots x states` matrix) exactly against
//!   the reference one-step argmin, and every returned power exactly against the oracle's choice.
//!
//! This is the check the engine will run periodically (`PLAN.md` §6): plan variants and the
//! periodic full differential are the only places the slow path is allowed.

// Test code: a failed assertion is the report, so `unwrap`/`expect`/`panic!` are the intended
// API here.  The workspace policy (no panics in shipped code) applies to the library, not to the
// tests that prove it refuses.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::float_cmp,
    clippy::manual_midpoint
)]

use std::path::PathBuf;

use energy_opt::validate::{self, Slot};
use energy_opt::{
    BatterySpec, Config, PvChargePreference, SolveInput, SolveResult, oracle, solve,
    solve_with_terminal_row,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    battery: Battery,
    start_kwh: f64,
    config: CaseConfig,
    slots: Slots,
    terminal_row: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Battery {
    e_min_kwh: f64,
    e_max_kwh: f64,
    p_charge_kw: f64,
    p_discharge_kw: f64,
    eta_charge: f64,
    eta_discharge: f64,
}

#[derive(Debug, Deserialize)]
struct CaseConfig {
    slot_minutes: f64,
    soc_step_kwh: f64,
    power_steps: i64,
    allow_grid_charge: bool,
    allow_export: bool,
    max_import_kw: f64,
    max_export_kw: f64,
    min_spread_eur_per_kwh: f64,
    wear_eur_per_kwh: f64,
    salvage: f64,
    target_penalty: f64,
    max_states: i64,
    max_iterations: i64,
    refuse_battery_export: bool,
    sell_floor_eur_per_kwh: Option<f64>,
    pv_charge_preference: String,
    e_target: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Slots {
    pv_kw: Vec<f64>,
    load_kw: Vec<f64>,
    buy: Vec<f64>,
    sell: Vec<f64>,
    minutes: Vec<f64>,
}

impl Case {
    fn config(&self) -> Config {
        let config = &self.config;
        Config {
            e_min_kwh: self.battery.e_min_kwh,
            e_max_kwh: self.battery.e_max_kwh,
            p_charge_kw: self.battery.p_charge_kw,
            p_discharge_kw: self.battery.p_discharge_kw,
            eta_charge: self.battery.eta_charge,
            eta_discharge: self.battery.eta_discharge,
            start_kwh: self.start_kwh,
            slot_minutes: config.slot_minutes,
            soc_step_kwh: config.soc_step_kwh,
            power_steps: config.power_steps,
            max_states: config.max_states,
            max_iterations: config.max_iterations,
            allow_grid_charge: config.allow_grid_charge,
            allow_export: config.allow_export,
            max_import_kw: config.max_import_kw,
            max_export_kw: config.max_export_kw,
            min_spread_eur_per_kwh: config.min_spread_eur_per_kwh,
            wear_eur_per_kwh: config.wear_eur_per_kwh,
            salvage: config.salvage,
            target_penalty: config.target_penalty,
            e_target: config.e_target,
            refuse_battery_export: config.refuse_battery_export,
            sell_floor_eur_per_kwh: config.sell_floor_eur_per_kwh,
            pv_charge_preference: match config.pv_charge_preference.as_str() {
                "prefer_pv" => PvChargePreference::PreferPv,
                "avoid_pv" => PvChargePreference::AvoidPv,
                _ => PvChargePreference::Auto,
            },
        }
    }

    fn terminal_row(&self, config: &Config) -> Option<Vec<f64>> {
        match self.terminal_row.as_deref() {
            None => None,
            Some("taper") => {
                let (grid, _) = validate::reference_grid(config, self.slots.pv_kw.len());
                Some(grid.iter().map(|energy| -0.04 * energy - 0.02).collect())
            }
            Some(other) => panic!("unknown terminal row kind in fixture: {other}"),
        }
    }

    fn input(&self) -> SolveInput<'_> {
        SolveInput::new(
            &self.slots.pv_kw,
            &self.slots.load_kw,
            &self.slots.buy,
            &self.slots.sell,
            &self.slots.minutes,
        )
    }
}

fn cases() -> Vec<Case> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parity.json");
    let fixture: Fixture =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the fixture is checked in"))
            .expect("the fixture parses");
    fixture.cases
}

fn close(got: f64, expected: f64) -> bool {
    (got - expected).abs() <= 1e-9 + 1e-12 * expected.abs()
}

#[test]
fn the_oracle_agrees_with_the_c_core() {
    for case in cases() {
        compare_case(&case);
    }
}

/// One fixture case: the C core through `solve()` against the brute-force oracle.
fn compare_case(case: &Case) {
    let config = case.config();
    let row = case.terminal_row(&config);
    let input = case.input();
    let solved = match row.as_deref() {
        Some(row) => solve_with_terminal_row(&input, &config, row),
        None => solve(&input, &config),
    }
    .unwrap_or_else(|error| panic!("case {} failed: {error}", case.name));
    let expected = oracle::solve(&config, &input, row.as_deref())
        .unwrap_or_else(|error| panic!("case {} oracle failed: {error}", case.name));

    assert_eq!(solved.iterations, expected.iterations, "{}", case.name);
    assert_eq!(solved.states, expected.states, "{}", case.name);
    let scalars = [
        ("objective", solved.objective_eur, expected.objective_eur),
        ("cycle cost", solved.cycle_cost_eur, expected.cycle_cost_eur),
        (
            "terminal error",
            solved.terminal_error_kwh,
            expected.terminal_error_kwh,
        ),
        (
            "internal objective",
            solved.internal_objective_eur,
            expected.internal_objective_eur,
        ),
        (
            "step",
            solved.effective_step_kwh,
            expected.effective_step_kwh,
        ),
    ];
    for (what, got, want) in scalars {
        assert!(close(got, want), "{}: {what} {got} != {want}", case.name);
    }
    compare_series(&solved, &expected, &case.name);
}

/// Every reported series, cell by cell.
fn compare_series(solved: &SolveResult, expected: &oracle::OracleResult, name: &str) {
    for (index, (got, want)) in solved.grid.iter().zip(&expected.grid).enumerate() {
        assert!(close(*got, *want), "{name}: grid[{index}]");
    }
    assert_eq!(solved.value.len(), expected.value.len(), "{name}");
    for (index, (got, want)) in solved.value.iter().zip(&expected.value).enumerate() {
        assert!(close(*got, *want), "{name}: value[{index}]");
    }
    for (index, (got, want)) in solved.energies.iter().zip(&expected.energies).enumerate() {
        assert!(close(*got, *want), "{name}: energies[{index}]");
    }
    for (index, (got, want)) in solved.powers.iter().zip(&expected.powers).enumerate() {
        assert!(close(*got, *want), "{name}: powers[{index}]");
    }
    for (index, (got, want)) in solved.windows.iter().zip(&expected.windows).enumerate() {
        for (a, b) in got.values().iter().zip(want.iter()) {
            assert!(close(*a, *b), "{name}: windows[{index}]");
        }
    }
}

#[test]
fn every_stored_decision_is_the_reference_argmin() {
    // `slots x states` decisions per case, compared exactly -- ties included.  This is the widest
    // statement available: not just the path, but every cell of the decision matrix.
    for case in cases() {
        let config = case.config();
        let row = case.terminal_row(&config);
        let input = case.input();
        let expected = oracle::solve(&config, &input, row.as_deref()).expect("oracle");
        let n = input.len();
        let states = expected.states;
        let mut checked = 0usize;

        for i in 0..n {
            let slot = Slot {
                pv_kw: input.pv_kw[i],
                load_kw: input.load_kw[i],
                buy: input.buy[i],
                sell: input.sell[i],
                minutes: input.minutes[i],
            };
            let dt = slot.minutes / 60.0;
            let next_row = &expected.value[(i + 1) * states..(i + 2) * states];
            for k in 0..states {
                let energy = expected.grid[k];
                let (lo, hi) = validate::bounds(&config, &slot, energy, dt);
                let extra = match (i + 1 == n, config.e_target) {
                    (true, Some(target)) => {
                        Some(validate::exact_power(&config, energy, target, dt))
                    }
                    _ => None,
                };
                let context =
                    validate::SlotContext::new(&config, slot, dt, next_row, &expected.grid);
                let (power, _cost) = validate::argmin_at(&context, energy, lo, hi, extra)
                    .expect("non-empty action set");
                assert_eq!(
                    power,
                    expected.argmin[i * states + k],
                    "{}: slot {i}, state {k}: oracle stored {} but the reference argmin is {power}",
                    case.name,
                    expected.argmin[i * states + k]
                );
                checked += 1;
            }
        }
        assert_eq!(checked, n * states);
        assert_eq!(expected.argmin.len(), n * states);
    }
}

#[test]
fn the_oracle_reproduces_a_known_scenario_by_hand() {
    // One slot, empty battery, free grid charge, no prices: charging to the ceiling is optimal and
    // the arithmetic is small enough to state exactly.
    let config = Config::new(
        BatterySpec {
            e_min_kwh: 0.0,
            e_max_kwh: 4.0,
            p_charge_kw: 2.0,
            p_discharge_kw: 2.0,
            eta_charge: 1.0,
            eta_discharge: 1.0,
        },
        0.0,
    );
    let mut config = config;
    config.soc_step_kwh = 0.5;
    let input = SolveInput::new(&[0.0], &[0.0], &[-0.1], &[0.0], &[60.0]);
    let solved: SolveResult = solve(&input, &config).expect("solve");
    let expected = oracle::solve(&config, &input, None).expect("oracle");
    assert_eq!(solved.powers, vec![2.0]);
    assert_eq!(expected.powers, vec![2.0]);
    // Charging 2 kWh at -0.1 EUR/kWh is a 0.2 EUR gain, and salvage is zero by default.
    assert!((solved.objective_eur - -0.2).abs() < 1e-12);
    assert!((expected.objective_eur - -0.2).abs() < 1e-12);
    assert_eq!(solved.energies, vec![0.0]);
}
