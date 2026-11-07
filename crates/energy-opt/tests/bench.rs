//! Performance gate: the 120-slot production solve must stay well inside the replan budget.
//!
//! The old add-on built a CPython extension specifically to get plan variants under ~20 ms; the
//! port needs the same for the 120 x 15-minute horizon at 398 states and 24 power steps.  Run with
//! `cargo test -p energy-opt --test bench -- --nocapture` to see the numbers.

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

use std::time::{Duration, Instant};

use energy_opt::{BatterySpec, Config, SolveInput, solve};

const SLOTS: usize = 120;
const STATES: usize = 398;
const POWER_STEPS: i64 = 24;

/// One production-shaped scenario: the config and its five flat arrays.
struct Scenario {
    config: Config,
    pv_kw: Vec<f64>,
    load_kw: Vec<f64>,
    buy: Vec<f64>,
    sell: Vec<f64>,
    minutes: Vec<f64>,
}

fn scenario() -> Scenario {
    let mut config = Config::new(
        BatterySpec {
            e_min_kwh: 1.536,
            e_max_kwh: 15.36,
            p_charge_kw: 3.2,
            p_discharge_kw: 3.2,
            eta_charge: 0.95,
            eta_discharge: 0.95,
        },
        7.0,
    );
    config.slot_minutes = 15.0;
    // The reference coarsens span/step to max_states - 3 = 397 intervals, then appends e_max.
    config.soc_step_kwh = 0.0348;
    config.power_steps = POWER_STEPS;
    config.wear_eur_per_kwh = 0.02;
    config.salvage = 0.05;

    let n = SLOTS as f64;
    let pv: Vec<f64> = (0..SLOTS)
        .map(|i| (std::f64::consts::PI * i as f64 / n).sin().max(0.0) * 6.0)
        .collect();
    let load: Vec<f64> = (0..SLOTS)
        .map(|i| 0.6 + 0.5 * (std::f64::consts::TAU * i as f64 / 96.0).cos())
        .collect();
    let buy: Vec<f64> = (0..SLOTS)
        .map(|i| 0.22 + 0.12 * (std::f64::consts::TAU * i as f64 / 96.0).sin())
        .collect();
    let sell = vec![0.06; SLOTS];
    let minutes = vec![15.0; SLOTS];
    Scenario {
        config,
        pv_kw: pv,
        load_kw: load,
        buy,
        sell,
        minutes,
    }
}

#[test]
fn a_120_slot_solve_stays_under_50_ms() {
    let scenario = scenario();
    let config = scenario.config;
    let input = SolveInput::new(
        &scenario.pv_kw,
        &scenario.load_kw,
        &scenario.buy,
        &scenario.sell,
        &scenario.minutes,
    );

    // One warm-up: `verify_core_build` is a `OnceLock` and the C pages are cold on the first call.
    let warm = solve(&input, &config).expect("the bench scenario solves");
    assert_eq!(
        warm.states, STATES,
        "the bench uses the production grid size"
    );

    let mut samples = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let result = solve(&input, &config).expect("the bench scenario solves");
        let elapsed = start.elapsed();
        assert_eq!(result.powers.len(), SLOTS);
        assert_eq!(result.states, STATES);
        samples.push(elapsed);
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    println!(
        "energy-opt bench: {SLOTS} slots x {STATES} states x {POWER_STEPS} power steps \
         ({} actions evaluated) -> median {:.2} ms, min {:.2} ms, max {:.2} ms",
        warm.iterations,
        median.as_secs_f64() * 1e3,
        samples[0].as_secs_f64() * 1e3,
        samples[samples.len() - 1].as_secs_f64() * 1e3,
    );
    if cfg!(debug_assertions) {
        // An unoptimised debug build is several times slower than the shipped release binary; the
        // gate is the release budget, so a debug run only reports its number.
        println!(
            "energy-opt bench: debug build (unoptimised), median {:.2} ms -- the < 50 ms budget is \
             asserted in release only",
            median.as_secs_f64() * 1e3
        );
    } else {
        assert!(
            median < Duration::from_millis(50),
            "solve took {:.2} ms (budget 50 ms)",
            median.as_secs_f64() * 1e3
        );
    }
}
