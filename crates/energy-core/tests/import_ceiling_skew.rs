//! The import-ceiling release must compare readings from the same instant.
//!
//! A meter read at the tick and a battery report that still shows the pre-ramp output describe two
//! different grids. Counting that as a release can restore the overload the guard was holding back.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::dispatch::{
    Dispatcher, FleetView, MeterReading, Overrides, PackReading, TickInput, TickOutcome,
};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeSink, NOW, pack, plan};

/// 5000 W limit, 200 W margin, 50 W deadband: a skewed 4500 W meter predicts a safe release.
fn limits() -> SafetyLimits {
    SafetyLimits {
        max_grid_import_w: 5_000.0,
        grid_import_margin_w: 200.0,
        grid_follow_deadband_w: 50.0,
        reserve_soc_pct: 10.0,
        ..SafetyLimits::default()
    }
}

struct Skew {
    plan: Plan,
    packs: Vec<PackReading>,
    dispatcher: Dispatcher,
}

impl Skew {
    fn new() -> Self {
        Self {
            plan: plan(NOW, 0.2, 0.96),
            packs: vec![pack("SN-A", 50.0), pack("SN-B", 50.0)],
            dispatcher: Dispatcher::new(limits(), true, 10),
        }
    }

    /// One tick with independently stamped meter and battery telemetry.
    async fn run(
        &mut self,
        meter_at: i64,
        grid_w: f64,
        pack_at: i64,
        pack_w: f64,
    ) -> (TickOutcome, FakeSink) {
        for reading in &mut self.packs {
            reading.observed_at = pack_at;
            reading.power_w = pack_w;
        }
        let meter = MeterReading::grid(grid_w).observed(meter_at);
        let fleet = FleetView::new(self.packs.clone());
        let overrides = Overrides::none();
        let sink = FakeSink::new();
        let input = TickInput {
            now: NOW,
            mode: Authority::Auto,
            plan: Some(&self.plan),
            fleet: &fleet,
            meter: &meter,
            latches: &[],
            overrides: &overrides,
            stale: false,
            progress: None,
            selection: None,
        };
        let outcome = self.dispatcher.tick(&input, &sink).await;
        (outcome, sink)
    }
}

#[tokio::test]
async fn skewed_meter_and_battery_telemetry_do_not_release_the_ceiling() {
    let mut scenario = Skew::new();
    let (breach, sink) = scenario.run(NOW, 6_000.0, NOW, 0.0).await;
    assert_eq!(
        breach.target_w, -1_200.0,
        "the breach is served by the fleet"
    );
    assert_eq!(sink.writes().iter().sum::<f64>(), -1_200.0);

    // The meter is fresh, but the pack report is 6 s old and has not seen the -1200 W ramp yet.
    // Contemporaneous values would predict 4500 + 200 - 0 = 4700 W and release into a charge;
    // with real skew that prediction is worthless.
    let (skewed, sink) = scenario.run(NOW, 4_500.0, NOW - 6, 0.0).await;
    assert_eq!(
        skewed.target_w, 0.0,
        "skew holds the measured 0 W output instead of releasing"
    );
    assert!(
        sink.writes().iter().all(|watts| *watts <= 0.0),
        "a charge must never be started on skewed telemetry: {:?}",
        sink.writes()
    );
    assert!(
        skewed
            .packs
            .iter()
            .any(|row| row.reason == Reason::GridImportCeiling),
        "the hold is still the guard's doing"
    );

    // Once the pack report catches up, the same low meter is a genuine release: the plan's charge
    // may run again.
    let (released, sink) = scenario.run(NOW, 4_500.0, NOW, 0.0).await;
    assert_eq!(
        released.target_w, 200.0,
        "aligned readings release the plan"
    );
    assert!(
        sink.writes().iter().all(|watts| *watts > 0.0),
        "the charge is restored: {:?}",
        sink.writes()
    );
}
