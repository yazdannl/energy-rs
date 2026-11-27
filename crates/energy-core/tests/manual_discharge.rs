//! A manual discharge is an ordinary negative setpoint: the override picks the direction, the
//! ladder, ceiling guards and device limits decide what may actually reach the pack.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assert_is_empty
)]

mod support;

use energy_core::dispatch::{
    Dispatcher, FleetView, ManualOverride, MeterReading, OverrideAction, Overrides, PackReading,
    TickInput, TickOutcome,
};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeSink, NOW, pack, plan};

fn discharge(pack_id: &str, watts: f64, until: i64) -> Overrides {
    Overrides::new(vec![ManualOverride::manual(
        pack_id,
        OverrideAction::Discharge { watts },
        until,
    )])
}

/// The tick inputs that vary between cases; the rest of the fixture stays the same.
struct Case<'a> {
    meter: &'a MeterReading,
    overrides: &'a Overrides,
    authority: Authority,
}

async fn tick(
    dispatcher: &Dispatcher,
    sink: &FakeSink,
    fleet: &[PackReading],
    planned: &Plan,
    now: i64,
    case: Case<'_>,
) -> TickOutcome {
    let view = FleetView::new(fleet.to_vec());
    let input = TickInput {
        now,
        mode: case.authority,
        plan: Some(planned),
        fleet: &view,
        meter: case.meter,
        latches: &[],
        overrides: case.overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    dispatcher.tick(&input, sink).await
}

fn negative_writes(sink: &FakeSink) -> Vec<f64> {
    sink.writes()
        .iter()
        .copied()
        .filter(|watts| *watts < 0.0)
        .collect()
}

#[tokio::test]
async fn a_manual_discharge_reaches_only_the_named_pack() {
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    let fleet = vec![pack("SN-A", 50.0), pack("SN-B", 50.0)];
    let sink = FakeSink::new();
    let overrides = discharge("SN-A", 400.0, NOW + 60);
    let outcome = tick(
        &dispatcher,
        &sink,
        &fleet,
        &plan(NOW, 0.0, 1.92),
        NOW,
        Case {
            meter: &MeterReading::default(),
            overrides: &overrides,
            authority: Authority::Auto,
        },
    )
    .await;
    let row = outcome.report("SN-A").expect("overridden row");
    assert_eq!(row.requested_w, -400.0, "watts is a positive magnitude");
    assert_eq!(row.intended_w, -400.0);
    assert_eq!(row.reason, Reason::ManualOverride);
    assert_eq!(negative_writes(&sink), vec![-400.0]);
    assert_eq!(outcome.report("SN-B").expect("peer").intended_w, 0.0);
}

#[tokio::test]
async fn an_expired_manual_discharge_returns_to_the_plan() {
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    let fleet = vec![pack("SN-A", 50.0)];
    let overrides = discharge("SN-A", 400.0, NOW + 30);
    assert!(overrides.active("SN-A", NOW + 30).is_some());
    assert!(overrides.active("SN-A", NOW + 31).is_none());

    let sink = FakeSink::new();
    let first = tick(
        &dispatcher,
        &sink,
        &fleet,
        &plan(NOW, 0.0, 0.96),
        NOW,
        Case {
            meter: &MeterReading::default(),
            overrides: &overrides,
            authority: Authority::Auto,
        },
    )
    .await;
    assert_eq!(first.report("SN-A").expect("live").intended_w, -400.0);

    let later = tick(
        &dispatcher,
        &FakeSink::new(),
        &fleet,
        &plan(NOW, 0.0, 0.96),
        NOW + 31,
        Case {
            meter: &MeterReading::default(),
            overrides: &overrides,
            authority: Authority::Auto,
        },
    )
    .await;
    let row = later.report("SN-A").expect("expired");
    assert_eq!(row.requested_w, 0.0, "the plan holds again");
    assert_eq!(row.intended_w, 0.0);
    assert_ne!(row.reason, Reason::ManualOverride);
}

#[tokio::test]
async fn a_manual_discharge_never_writes_outside_auto() {
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    let sink = FakeSink::new();
    let overrides = discharge("SN-A", 400.0, NOW + 60);
    let outcome = tick(
        &dispatcher,
        &sink,
        &[pack("SN-A", 50.0)],
        &plan(NOW, 0.0, 0.96),
        NOW,
        Case {
            meter: &MeterReading::default(),
            overrides: &overrides,
            authority: Authority::DryRun,
        },
    )
    .await;
    assert_eq!(
        outcome.report("SN-A").expect("row").reason,
        Reason::ModeNotAuto
    );
    assert_eq!(sink.writes(), Vec::<f64>::new());
}

#[tokio::test]
async fn an_export_ceiling_still_clamps_a_manual_discharge() {
    let limits = SafetyLimits {
        max_grid_export_w: 300.0,
        ..SafetyLimits::default()
    };
    let dispatcher = Dispatcher::new(limits, true, 10);
    let sink = FakeSink::new();
    let overrides = discharge("SN-A", 800.0, NOW + 60);
    // No modelled house load, so the 300 W export ceiling really is 300 W of battery discharge.
    let mut planned = plan(NOW, 0.0, 0.96);
    for slot in &mut planned.slots {
        slot.load_kw = 0.0;
    }
    let outcome = tick(
        &dispatcher,
        &sink,
        &[pack("SN-A", 50.0)],
        &planned,
        NOW,
        Case {
            meter: &MeterReading::default(),
            overrides: &overrides,
            authority: Authority::Auto,
        },
    )
    .await;
    let row = outcome.report("SN-A").expect("row");
    assert_eq!(row.requested_w, -800.0);
    assert_eq!(
        row.intended_w, -300.0,
        "the site export bound outranks the manual request"
    );
    assert_eq!(row.reason, Reason::GridExportLimit);
}

#[tokio::test]
async fn the_import_ceiling_guard_keeps_its_authority_over_a_manual_discharge() {
    let limits = SafetyLimits {
        max_grid_import_w: 7_700.0,
        grid_import_margin_w: 200.0,
        max_grid_export_w: 1_500.0,
        ..SafetyLimits::default()
    };
    let dispatcher = Dispatcher::new(limits, true, 10);
    let fleet = vec![pack("SN-A", 50.0), pack("SN-B", 50.0)];
    let sink = FakeSink::new();
    let overrides = discharge("SN-A", 800.0, NOW + 60);
    // A netted slot keeps the grid follow out of the way: only the guard moves the fleet target.
    let mut planned = plan(NOW, 0.0, 1.92);
    for slot in &mut planned.slots {
        slot.buy_all_in = 0.415_838;
        slot.feed_in = 0.415_838;
    }
    let outcome = tick(
        &dispatcher,
        &sink,
        &fleet,
        &planned,
        NOW,
        Case {
            meter: &MeterReading::grid(7_600.0).observed(NOW),
            overrides: &overrides,
            authority: Authority::Auto,
        },
    )
    .await;
    assert_eq!(
        outcome.target_w, -100.0,
        "the measured breach still arms the guard"
    );
    assert_eq!(outcome.report("SN-A").expect("override").intended_w, -800.0);
    assert_eq!(
        outcome.report("SN-B").expect("peer").intended_w,
        0.0,
        "the override never becomes a new import"
    );
    assert!(negative_writes(&sink).iter().all(|watts| *watts >= -800.0));
}

#[tokio::test]
async fn the_soc_reserve_still_clamps_a_manual_discharge() {
    let limits = SafetyLimits {
        reserve_soc_pct: 15.0,
        ..SafetyLimits::default()
    };
    let dispatcher = Dispatcher::new(limits, true, 10);
    let sink = FakeSink::new();
    let overrides = discharge("SN-A", 800.0, NOW + 60);
    let outcome = tick(
        &dispatcher,
        &sink,
        &[pack("SN-A", 10.0)],
        &plan(NOW, 0.0, 0.96),
        NOW,
        Case {
            meter: &MeterReading::default(),
            overrides: &overrides,
            authority: Authority::Auto,
        },
    )
    .await;
    let row = outcome.report("SN-A").expect("row");
    assert_eq!(row.requested_w, -800.0);
    assert_eq!(row.intended_w, 0.0, "below-reserve packs are protected");
    assert_eq!(row.reason, Reason::SocFloor);
    assert_eq!(negative_writes(&sink), Vec::<f64>::new());
}
