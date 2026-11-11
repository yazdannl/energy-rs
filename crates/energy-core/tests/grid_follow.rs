//! The grid follow: the meter moves, the target moves with it, the plan stays.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::dispatch::MeterReading;
use energy_core::safety::Authority;
use support::fakes::{NOW, pack, plan};
use support::harness;

/// The fixture's slot plans a quiet house (load 0.5 kW, no PV) and holds the battery, so the plan is
/// deliberately not exchanging energy and the site should hold the meter at zero.
const CALM_GRID_W: f64 = 0.0;

fn fixture(grid_w: f64) -> harness::Harness {
    let h = harness::build(
        Authority::DryRun,
        // 25 % of two 1.92 kWh packs is the 0.96 kWh the slot plans for, so the SoC drift trigger
        // stays quiet and only the follow moves the target.
        vec![pack("SN-A", 25.0), pack("SN-B", 25.0)],
        plan(NOW, 0.0, 0.96),
    );
    h.meter.set(MeterReading::grid(grid_w));
    h
}

/// The fingerprinted target, or `None` when the tick carried no plan power.
fn target(h: &harness::Harness, outcome: &energy_core::dispatch::TickOutcome) -> f64 {
    let _ = h;
    outcome.target_w
}

#[tokio::test]
async fn a_load_step_is_matched_within_one_tick() {
    let h = fixture(CALM_GRID_W);
    h.engine.boot(NOW).await.expect("boot");
    let calm = h.engine.tick_once(NOW).await.expect("calm tick");
    assert_eq!(target(&h, &calm), 0.0, "nothing to correct");

    // The house starts drawing 2 kW instead of 500 W: the fleet must discharge to cover it.
    h.meter.set(MeterReading::grid(2_000.0));
    let stepped = h.engine.tick_once(NOW + 1).await.expect("stepped tick");
    // Two 800 W packs can deliver 1600 W, so 400 W of the step still comes from the grid.
    assert_eq!(stepped.target_w, -1_600.0, "discharge what the fleet has");
    assert!(
        (stepped
            .packs
            .iter()
            .map(|pack| pack.intended_w)
            .sum::<f64>()
            + 1_600.0)
            .abs()
            < 1.0
    );
}

#[tokio::test]
async fn the_plan_is_not_re_solved_for_a_power_step() {
    let h = fixture(CALM_GRID_W);
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("calm tick");
    let solves = h.source.calls();
    h.meter.set(MeterReading::grid(2_000.0));
    h.engine.tick_once(NOW + 1).await.expect("stepped tick");
    assert_eq!(h.source.calls(), solves, "the follow must not replan");
}

#[tokio::test]
async fn a_step_inside_the_deadband_changes_nothing() {
    let h = fixture(CALM_GRID_W);
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("calm tick");
    h.meter.set(MeterReading::grid(CALM_GRID_W + 9.0));
    let tick = h.engine.tick_once(NOW + 1).await.expect("small tick");
    assert_eq!(tick.target_w, 0.0, "9 W is noise");
}

#[tokio::test]
async fn no_grid_reading_means_no_follow() {
    let h = fixture(CALM_GRID_W);
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("calm tick");
    h.meter.set(MeterReading::default());
    let tick = h.engine.tick_once(NOW + 1).await.expect("blind tick");
    assert_eq!(
        tick.target_w, 0.0,
        "without a meter there is nothing to follow"
    );
}

#[tokio::test]
async fn a_surplus_is_absorbed_rather_than_exported() {
    // Planned: load 0.5 kW, no PV. Reality: 1.5 kW of PV, so the grid exports 1 kW.
    let h = fixture(CALM_GRID_W);
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("calm tick");
    h.meter.set(MeterReading {
        grid_w: Some(-1_000.0),
        load_kw: Some(0.5),
        pv_kw: Some(1.5),
    });
    let tick = h.engine.tick_once(NOW + 1).await.expect("surplus tick");
    assert_eq!(
        tick.target_w, 1_000.0,
        "charge the 1 kW of surplus, and do not import to match the forecast's 500 W"
    );
}

#[tokio::test]
async fn a_deliberate_grid_charge_keeps_its_import() {
    // The slot plans to import 200 W to charge 300 W (load 500 W): that import is a decision, so
    // the house drawing more must not cancel it.
    let h = harness::build(
        Authority::DryRun,
        vec![pack("SN-A", 25.0), pack("SN-B", 25.0)],
        plan(NOW, 0.3, 0.96),
    );
    h.meter.set(MeterReading::grid(700.0));
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    let tick = h.engine.tick_once(NOW + 1).await.expect("second tick");
    // Planned grid 200 W, measured 700 W: the fleet takes 500 W out of the import, keeping 200 W.
    assert!(
        (tick.target_w + 200.0).abs() < 1.0,
        "keep the planned 200 W import, got {}",
        tick.target_w
    );
}
