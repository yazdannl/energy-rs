//! A measured power move buys a solve only after it holds, and never two in a row.

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

/// The plan's slot expects a quiet house (load 0.5 kW, no PV, no battery), so the grid should be
/// 500 W; the meter is set to a different number to script a deviation.
fn fixture(grid_w: f64) -> harness::Harness {
    let h = harness::build(
        Authority::DryRun,
        // 25 % of two 1.92 kWh packs is the 0.96 kWh the fixture's slot plans for, so the SoC
        // deviation trigger stays quiet and only the power move can buy a solve.
        vec![pack("SN-A", 25.0), pack("SN-B", 25.0)],
        plan(NOW, 0.0, 0.96),
    );
    h.meter.set(MeterReading::grid(grid_w));
    h
}

fn solves(h: &harness::Harness) -> usize {
    h.source.calls()
}

#[tokio::test]
async fn a_steady_step_confirms_and_solves() {
    let h = fixture(2_500.0);
    h.engine.boot(NOW).await.expect("boot");
    // The first tick solves because there is no plan yet; the step then has to hold for two ticks
    // before it buys a second solve.
    h.engine.tick_once(NOW).await.expect("first tick");
    h.engine.tick_once(NOW + 5).await.expect("second tick");
    h.engine.tick_once(NOW + 10).await.expect("third tick");
    assert!(solves(&h) >= 2, "solves: {}", solves(&h));
}

#[tokio::test]
async fn a_single_spike_does_not_solve() {
    let h = fixture(500.0);
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    let before = solves(&h);
    // One tick with a 2 kW deviation, then back to the plan's own number.
    h.meter.set(MeterReading::grid(2_500.0));
    h.engine.tick_once(NOW + 5).await.expect("spike tick");
    h.meter.set(MeterReading::grid(500.0));
    h.engine.tick_once(NOW + 10).await.expect("settled tick");
    h.engine.tick_once(NOW + 15).await.expect("quiet tick");
    assert_eq!(solves(&h), before, "a spike must not buy a solve");
}
