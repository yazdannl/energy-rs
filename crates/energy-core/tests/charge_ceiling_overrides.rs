//! Overrides at the charge ceiling: the effective request, not the split, decides direction.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::assert_is_empty
)]

mod support;

use energy_battery::SocLimit;
use energy_core::dispatch::{
    Dispatcher, FleetView, ManualOverride, MeterReading, OverrideAction, Overrides, PackReading,
    TickInput, TickOutcome,
};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeSink, NOW, Sent, pack, plan};

fn full(id: &str) -> PackReading {
    let mut reading = pack(id, 100.0);
    reading.spec.max_soc_pct = 100.0;
    reading.device_soc_pct = Some(100.0);
    reading.soc_limit = SocLimit::Upper;
    reading.fault_level = Some(2);
    reading.energy_kwh = Some(reading.spec.capacity_kwh);
    reading
}

fn able(id: &str, soc_pct: f64) -> PackReading {
    let mut reading = pack(id, soc_pct);
    reading.spec.max_soc_pct = 100.0;
    reading.device_soc_pct = Some(soc_pct);
    reading.energy_kwh = Some(reading.spec.capacity_kwh * soc_pct / 100.0);
    reading
}

fn plan_at(watts: f64, soc_kwh: f64) -> Plan {
    let mut planned = plan(NOW, watts / 1000.0, soc_kwh);
    for slot in &mut planned.slots {
        slot.soc_kwh = soc_kwh;
        slot.target_soc_kwh = soc_kwh + watts / 1000.0 * 0.25;
    }
    planned
}

fn limits() -> SafetyLimits {
    SafetyLimits {
        ramp_limit_w: 10_000.0,
        ..SafetyLimits::default()
    }
}

async fn tick(
    dispatcher: &Dispatcher,
    sink: &FakeSink,
    fleet: &[PackReading],
    planned: &Plan,
    now: i64,
    overrides: &Overrides,
) -> TickOutcome {
    let view = FleetView::new(fleet.to_vec());
    let input = TickInput {
        now,
        mode: Authority::Auto,
        plan: Some(planned),
        fleet: &view,
        meter: &MeterReading::default(),
        latches: &[],
        overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    dispatcher.tick(&input, sink).await
}

/// Latched once under a positive plan, so the next tick has a real charge ceiling verdict.
async fn latch(dispatcher: &Dispatcher, fleet: &[PackReading]) {
    tick(
        dispatcher,
        &FakeSink::new(),
        fleet,
        &plan_at(1600.0, 20.0),
        NOW,
        &Overrides::none(),
    )
    .await;
}

fn no_write_to(sink: &FakeSink, pack_id: &str) -> bool {
    sink.commands().iter().all(|(id, _)| id != pack_id)
}

fn negative_write_to(sink: &FakeSink, pack_id: &str) -> bool {
    sink.commands()
        .iter()
        .any(|(id, sent)| id == pack_id && matches!(sent, Sent::Write(watts) if *watts < 0.0))
}

/// A manual charge override cannot re-authorise a ceiling-latched pack under a negative tick.
#[tokio::test]
async fn a_manual_charge_override_cannot_overcharge_a_ceiling_latched_pack() {
    let fleet = vec![full("SN-FULL"), able("SN-A", 97.0)];
    let dispatcher = Dispatcher::new(limits(), true, 10);
    latch(&dispatcher, &fleet).await;

    let sink = FakeSink::new();
    let overrides = Overrides::new(vec![ManualOverride::manual(
        "SN-FULL",
        OverrideAction::Charge { watts: 800.0 },
        NOW + 30,
    )]);
    let outcome = tick(
        &dispatcher,
        &sink,
        &fleet,
        &plan_at(-1600.0, 3.7),
        NOW + 5,
        &overrides,
    )
    .await;
    let row = outcome.report("SN-FULL").expect("full row");
    assert_eq!(row.requested_w, 0.0);
    assert_eq!(row.intended_w, 0.0);
    assert_eq!(row.reason, Reason::ChargeAtCeiling);
    assert!(row.capability_skipped);
    assert!(
        row.note
            .as_deref()
            .is_some_and(|note| note.contains("ceiling")),
        "the blocked charge explains itself: {:?}",
        row.note
    );
    assert!(no_write_to(&sink, "SN-FULL"), "{:?}", sink.commands());
}

/// The maintenance full-charge path is a charge request too, even under a negative tick target.
#[tokio::test]
async fn a_maintenance_charge_override_cannot_overcharge_a_ceiling_latched_pack() {
    let fleet = vec![full("SN-FULL"), able("SN-A", 97.0)];
    let dispatcher = Dispatcher::new(limits(), true, 10);
    latch(&dispatcher, &fleet).await;

    let sink = FakeSink::new();
    let overrides = Overrides::new(vec![ManualOverride::maintenance(
        "SN-FULL",
        OverrideAction::Charge { watts: 800.0 },
        NOW + 30,
    )]);
    let outcome = tick(
        &dispatcher,
        &sink,
        &fleet,
        &plan_at(-1600.0, 3.7),
        NOW + 5,
        &overrides,
    )
    .await;
    let row = outcome.report("SN-FULL").expect("full row");
    assert_eq!(row.requested_w, 0.0);
    assert_eq!(row.intended_w, 0.0);
    assert_eq!(row.reason, Reason::ChargeAtCeiling);
    assert!(row.capability_skipped);
    assert!(
        row.note
            .as_deref()
            .is_some_and(|note| note.contains("ceiling")),
        "{:?}",
        row.note
    );
    assert!(no_write_to(&sink, "SN-FULL"), "{:?}", sink.commands());
}

/// A discharge override is not a charge, even when the plan and allocation ask to charge.
#[tokio::test]
async fn a_manual_discharge_override_can_use_a_ceiling_latched_pack() {
    let fleet = vec![full("SN-FULL"), able("SN-A", 60.0)];
    let dispatcher = Dispatcher::new(limits(), true, 10);
    latch(&dispatcher, &fleet).await;

    let sink = FakeSink::new();
    let overrides = Overrides::new(vec![ManualOverride::manual(
        "SN-FULL",
        OverrideAction::Charge { watts: -800.0 },
        NOW + 30,
    )]);
    let outcome = tick(
        &dispatcher,
        &sink,
        &fleet,
        &plan_at(1600.0, 20.0),
        NOW + 5,
        &overrides,
    )
    .await;
    let row = outcome.report("SN-FULL").expect("full row");
    assert!(
        row.requested_w < 0.0 && row.intended_w < 0.0,
        "requested={} intended={} reason={:?}",
        row.requested_w,
        row.intended_w,
        row.reason
    );
    assert_eq!(row.reason, Reason::ManualOverride);
    assert_eq!(row.requested_w, -800.0);
    assert!(!row.capability_skipped);
    assert!(
        !row.note
            .as_deref()
            .is_some_and(|note| note.contains("ceiling")),
        "{:?}",
        row.note
    );
    assert!(
        negative_write_to(&sink, "SN-FULL"),
        "the discharge override must reach the device: {:?}",
        sink.commands()
    );
}
