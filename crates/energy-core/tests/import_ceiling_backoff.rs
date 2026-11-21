//! The import ceiling is safety traffic: a control backoff must not delay it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

mod support;

use energy_core::dispatch::{
    Dispatcher, FleetView, LONG_FAULT_BACKOFF_S, MeterReading, Overrides, TickInput,
};
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeSink, NOW, pack, plan};

#[tokio::test]
async fn a_measured_breach_is_dispatched_even_while_every_pack_is_backing_off() {
    let mut plan = plan(NOW, 0.0, 1.92);
    for slot in &mut plan.slots {
        slot.buy_all_in = 0.30;
        slot.feed_in = 0.30;
        slot.load_kw = 0.0;
        slot.pv_kw = 0.0;
    }
    let limits = SafetyLimits {
        max_grid_import_w: 7_700.0,
        grid_import_margin_w: 200.0,
        reserve_soc_pct: 10.0,
        ..SafetyLimits::default()
    };
    let dispatcher = Dispatcher::new(limits.clone(), true, 10);
    dispatcher.faults().arm("SN-A", NOW, "scripted fault");
    dispatcher.faults().arm("SN-B", NOW, "scripted fault");
    assert!(dispatcher.faults().blocking("SN-A", NOW));

    let fleet = FleetView::new(vec![pack("SN-A", 50.0), pack("SN-B", 50.0)]);
    let meter = MeterReading {
        observed_at: Some(NOW),
        ..MeterReading::grid(7_600.0)
    };
    let overrides = Overrides::none();
    let sink = FakeSink::new();
    let outcome = dispatcher
        .tick(
            &TickInput {
                now: NOW,
                mode: Authority::Auto,
                plan: Some(&plan),
                fleet: &fleet,
                meter: &meter,
                latches: &[],
                overrides: &overrides,
                stale: false,
                progress: None,
                selection: None,
            },
            &sink,
        )
        .await;

    assert_eq!(outcome.target_w, -100.0);
    assert!(
        !sink.writes().is_empty(),
        "the import ceiling must not wait out a control backoff"
    );
    assert!(
        outcome
            .packs
            .iter()
            .any(|row| row.reason == Reason::GridImportCeiling)
    );
}

#[tokio::test]
async fn a_partial_safety_write_does_not_shorten_or_relabel_a_real_fault() {
    let mut plan = plan(NOW, 0.0, 1.92);
    for slot in &mut plan.slots {
        slot.buy_all_in = 0.30;
        slot.feed_in = 0.30;
        slot.load_kw = 0.0;
        slot.pv_kw = 0.0;
    }
    let limits = SafetyLimits {
        max_grid_import_w: 7_700.0,
        grid_import_margin_w: 200.0,
        reserve_soc_pct: 10.0,
        ..SafetyLimits::default()
    };
    let dispatcher = Dispatcher::new(limits.clone(), true, 10);
    dispatcher
        .faults()
        .arm_long("SN-A", NOW, "scripted no-flow");
    dispatcher
        .faults()
        .arm_long("SN-B", NOW, "scripted no-flow");
    let fleet = FleetView::new(vec![pack("SN-A", 50.0), pack("SN-B", 50.0)]);
    let meter = MeterReading {
        observed_at: Some(NOW),
        ..MeterReading::grid(7_600.0)
    };
    let overrides = Overrides::none();
    let sink = FakeSink::new();
    sink.curtail(45);
    dispatcher
        .tick(
            &TickInput {
                now: NOW + 5,
                mode: Authority::Auto,
                plan: Some(&plan),
                fleet: &fleet,
                meter: &meter,
                latches: &[],
                overrides: &overrides,
                stale: false,
                progress: None,
                selection: None,
            },
            &sink,
        )
        .await;

    assert!(
        !sink.writes().is_empty(),
        "safety still bypasses the backoff"
    );
    assert_eq!(dispatcher.faults().episode_start("SN-A"), Some(NOW));
    let cause = dispatcher
        .faults()
        .blocker("SN-A", NOW + LONG_FAULT_BACKOFF_S - 1)
        .expect("the real fault must still be active");
    assert_eq!(cause, "scripted no-flow");
}
