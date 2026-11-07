//! The fleet bounds must hold for the FINAL command vector, not just for the allocation.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assert_is_empty,
    clippy::manual_assert_eq,
    clippy::assertions_on_constants
)]

mod support;

use chrono::{TimeZone, Utc};
use energy_battery::protection::{CellLatch, LatchReason};
use energy_core::dispatch::{
    Dispatcher, FleetView, ManualOverride, MeterReading, OverrideAction, Overrides, PackReading,
    TickInput, TickOutcome,
};
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeSink, NOW, pack, plan};

fn latch(pack_id: &str, force_watts: Option<u32>) -> CellLatch {
    let at = Utc.timestamp_opt(0, 0).single().expect("epoch");
    CellLatch {
        pack_id: pack_id.to_owned(),
        blocked: true,
        force_charge: force_watts.is_some(),
        force_charge_w: force_watts,
        since: at,
        reason: LatchReason::BelowCutoff,
        reason_at: at,
        voltage_v: Some(2.9),
        last_fresh_at: Some(at),
    }
}

fn charge(pack_id: &str, watts: f64) -> ManualOverride {
    ManualOverride::manual(pack_id, OverrideAction::Charge { watts }, NOW + 300)
}

async fn tick(
    packs: Vec<PackReading>,
    overrides: Vec<ManualOverride>,
    latches: Vec<CellLatch>,
    limits: SafetyLimits,
) -> (TickOutcome, FakeSink) {
    // A quiet site: no base load and no PV, so the site limit is all the batteries' budget.
    let mut plan = plan(NOW, 0.0, 1.92);
    for slot in &mut plan.slots {
        slot.load_kw = 0.0;
        slot.pv_kw = 0.0;
    }
    let fleet = FleetView::new(packs);
    let meter = MeterReading::default();
    let overrides = Overrides::new(overrides);
    let sink = FakeSink::new();
    let dispatcher = Dispatcher::new(limits, true, 10);
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(&plan),
        fleet: &fleet,
        meter: &meter,
        latches: &latches,
        overrides: &overrides,
        stale: false,
        progress: None,
    };
    let outcome = dispatcher.tick(&input, &sink).await;
    (outcome, sink)
}

fn four() -> Vec<PackReading> {
    ["SN-A", "SN-B", "SN-C", "SN-D"]
        .iter()
        .map(|id| pack(id, 50.0))
        .collect()
}

/// Four emergency charges of 700 W are 2800 W: the site's 1000 W must still bind.
#[tokio::test]
async fn a_forced_charge_cannot_break_the_site_limit() {
    let latches: Vec<CellLatch> = ["SN-A", "SN-B", "SN-C", "SN-D"]
        .iter()
        .map(|id| latch(id, Some(700)))
        .collect();
    let (outcome, sink) = tick(
        four(),
        Vec::new(),
        latches,
        SafetyLimits {
            max_grid_import_w: 1000.0,
            ..SafetyLimits::default()
        },
    )
    .await;
    assert_eq!(outcome.fleet_clamp, Some(Reason::GridImportLimit));
    let written: f64 = sink.writes().iter().sum();
    assert!(
        (written - 1000.0).abs() < 5.0,
        "the final vector wrote {written} W"
    );
    for report in &outcome.packs {
        assert!(
            (report.intended_w - 250.0).abs() < 5.0,
            "{} kept {} W",
            report.pack_id,
            report.intended_w
        );
        assert_eq!(report.reason, Reason::GridImportLimit);
    }
}

/// Four 700 W manual overrides are 2800 W: the fleet charge budget must still bind.
#[tokio::test]
async fn an_override_cannot_break_the_fleet_power_budget() {
    let overrides = ["SN-A", "SN-B", "SN-C", "SN-D"]
        .iter()
        .map(|id| charge(id, 700.0))
        .collect();
    let (outcome, sink) = tick(
        four(),
        overrides,
        Vec::new(),
        SafetyLimits {
            fleet_max_charge_w: 1000.0,
            ..SafetyLimits::default()
        },
    )
    .await;
    assert_eq!(outcome.fleet_clamp, Some(Reason::PowerClamp));
    let written: f64 = sink.writes().iter().sum();
    assert!(
        (written - 1000.0).abs() < 5.0,
        "the final vector wrote {written} W"
    );
    assert!(
        outcome
            .packs
            .iter()
            .all(|report| report.reason == Reason::PowerClamp)
    );
}

/// A pack the tick cannot command keeps executing 700 W; that is part of the site's budget.
#[tokio::test]
async fn a_pack_that_is_not_written_keeps_its_power_in_the_budget() {
    let mut gone = pack("SN-B", 50.0);
    gone.available = false;
    gone.power_w = 700.0;
    let (outcome, sink) = tick(
        vec![pack("SN-A", 50.0), gone],
        vec![charge("SN-A", 800.0)],
        Vec::new(),
        SafetyLimits {
            max_grid_import_w: 1000.0,
            ..SafetyLimits::default()
        },
    )
    .await;
    assert_eq!(outcome.fleet_clamp, Some(Reason::GridImportLimit));
    assert_eq!(
        sink.writes(),
        vec![300.0],
        "only 300 W was left under the site limit"
    );
    assert!(
        sink.commands().iter().all(|(pack_id, _)| pack_id != "SN-B"),
        "an unavailable pack must not be written by the dispatcher"
    );
    assert_eq!(outcome.report("SN-A").expect("report").intended_w, 300.0);
}

/// A hold slot with no override keeps the vector at zero: the clamp changes nothing.
#[tokio::test]
async fn a_quiet_tick_is_not_clamped() {
    let (outcome, sink) = tick(
        four(),
        Vec::new(),
        Vec::new(),
        SafetyLimits {
            max_grid_import_w: 1000.0,
            ..SafetyLimits::default()
        },
    )
    .await;
    assert_eq!(outcome.fleet_clamp, None);
    assert!(sink.writes().is_empty());
    assert_eq!(sink.holds(), 4, "every pack gets its neutral hold");
}
