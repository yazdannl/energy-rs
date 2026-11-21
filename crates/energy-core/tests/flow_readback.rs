//! The readback stays authoritative on every write path, overrides included.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_battery::ControlFault;
use energy_core::dispatch::{
    FleetView, LONG_FAULT_BACKOFF_S, MeterReading, OverrideAction, PackFlow,
};
use energy_core::safety::Authority;

use support::fakes::{NOW, Sent, pack, plan};
use support::harness;

fn silent_pack() -> energy_core::dispatch::PackReading {
    let mut reading = pack("SN-A", 12.0);
    reading.flow = Some(PackFlow {
        grid_input_w: 0,
        output_pack_w: 0,
        output_home_w: 0,
        ac_status: Some(0),
        dc_status: Some(0),
    });
    reading
}

#[tokio::test]
async fn an_override_that_echoes_but_draws_nothing_is_faulted_and_held_for_minutes() {
    let h = harness::build(Authority::Auto, vec![silent_pack()], plan(NOW, 1.0, 0.96));
    h.engine.boot(NOW).await.expect("boot");
    h.engine
        .override_pack("SN-A", OverrideAction::Charge { watts: 1000.0 }, 600, NOW)
        .expect("override");
    for offset in 1..=3 {
        h.engine
            .tick_once(NOW + 5 * offset)
            .await
            .expect("tick with no flow");
    }
    let cause = h
        .engine
        .dispatcher()
        .faults()
        .blocker("SN-A", NOW + 20)
        .expect("a no-flow fault must block the pack");
    assert!(
        cause.contains("no AC power flowed"),
        "the readback must name the missing flow: {cause}"
    );
    assert!(
        h.engine
            .dispatcher()
            .faults()
            .blocker("SN-A", NOW + LONG_FAULT_BACKOFF_S - 1)
            .is_some(),
        "the backoff lasts minutes, not one tick"
    );

    let before = h.sink.commands().len();
    h.engine.tick_once(NOW + 21).await.expect("backoff tick");
    let after: Vec<_> = h.sink.commands().into_iter().skip(before).collect();
    assert!(
        !after
            .iter()
            .any(|(pack_id, sent)| { pack_id == "SN-A" && !matches!(sent, Sent::Hold) }),
        "a no-flow pack must not be re-commanded: {after:?}"
    );

    let snapshot = h.engine.snapshot(
        NOW + 21,
        &FleetView::new(vec![silent_pack()]),
        &MeterReading::default(),
    );
    assert!(
        snapshot.packs[0]
            .control_fault
            .as_deref()
            .is_some_and(|fault| fault.contains("no AC power flowed")),
        "the fault must reach the API snapshot: {:?}",
        snapshot.packs[0]
    );
}

#[tokio::test]
async fn a_message_id_reset_arms_a_long_backoff_without_a_hold() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, 1.0, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("settle");
    let before = h.sink.len();
    h.sink.fault_with(
        "SN-A",
        ControlFault::Restarted {
            sn: "SN-A".to_owned(),
            before: 187,
            after: 10,
        },
    );
    h.engine.tick_once(NOW + 5).await.expect("reset tick");

    let after: Vec<_> = h.sink.commands().into_iter().skip(before).collect();
    assert!(
        !after.iter().any(|(_, sent)| matches!(sent, Sent::Hold)),
        "a reset gets no fail-safe hold: {after:?}"
    );
    let cause = h
        .engine
        .dispatcher()
        .faults()
        .blocker("SN-A", NOW + 100)
        .expect("a reset must block the pack");
    assert!(cause.contains("restarted"), "unexpected cause: {cause}");
    assert!(
        h.engine
            .dispatcher()
            .faults()
            .blocker("SN-A", NOW + LONG_FAULT_BACKOFF_S - 1)
            .is_some()
    );
}

fn flowing_pack() -> energy_core::dispatch::PackReading {
    let mut reading = pack("SN-A", 12.0);
    reading.flow = Some(PackFlow {
        grid_input_w: 1000,
        output_pack_w: 0,
        output_home_w: 0,
        ac_status: Some(2),
        dc_status: Some(1),
    });
    reading
}

#[tokio::test]
async fn a_backoff_expires_without_rearming_an_unchanged_streak() {
    let h = harness::build(Authority::Auto, vec![silent_pack()], plan(NOW, 1.0, 0.96));
    h.engine.boot(NOW).await.expect("boot");
    h.engine
        .override_pack("SN-A", OverrideAction::Charge { watts: 1000.0 }, 600, NOW)
        .expect("override");
    for offset in 1..=3 {
        h.engine
            .tick_once(NOW + 5 * offset)
            .await
            .expect("fault tick");
    }
    assert!(
        h.engine
            .dispatcher()
            .faults()
            .blocking("SN-A", NOW + LONG_FAULT_BACKOFF_S - 1)
    );

    h.engine
        .set_mode(Authority::Advisory, NOW + 16)
        .await
        .expect("advisory");
    h.engine
        .tick_once(NOW + LONG_FAULT_BACKOFF_S + 20)
        .await
        .expect("expiry tick");
    assert!(
        !h.engine
            .dispatcher()
            .faults()
            .blocking("SN-A", NOW + LONG_FAULT_BACKOFF_S + 21),
        "the old streak must not extend the same fault episode"
    );
}

#[tokio::test]
async fn fresh_compliant_flow_clears_the_streak_without_a_new_write() {
    let h = harness::build(Authority::Auto, vec![silent_pack()], plan(NOW, 1.0, 0.96));
    h.engine.boot(NOW).await.expect("boot");
    h.engine
        .override_pack("SN-A", OverrideAction::Charge { watts: 1000.0 }, 600, NOW)
        .expect("override");
    for offset in 1..=3 {
        h.engine
            .tick_once(NOW + 5 * offset)
            .await
            .expect("fault tick");
    }
    h.fleet.set(vec![flowing_pack()]);
    let before = h.sink.len();
    h.engine.tick_once(NOW + 20).await.expect("recovery tick");
    assert_eq!(h.sink.len(), before, "backoff must suppress new writes");
    h.engine
        .tick_once(NOW + LONG_FAULT_BACKOFF_S + 20)
        .await
        .expect("expiry after recovery");
    assert!(
        !h.engine
            .dispatcher()
            .faults()
            .blocking("SN-A", NOW + LONG_FAULT_BACKOFF_S + 21),
        "recovery evidence must end the suppression"
    );
}

#[tokio::test]
async fn a_status_only_discharge_disagreement_is_visible_without_faulting() {
    let mut reading = pack("SN-A", 50.0);
    reading.flow = Some(PackFlow {
        grid_input_w: 0,
        output_pack_w: 0,
        output_home_w: 300,
        ac_status: Some(2),
        dc_status: Some(0),
    });
    reading.flow_alarm = reading.flow.and_then(|flow| flow.discharge_status_alarm());
    assert!(reading.flow_alarm.is_some());
    assert!(!reading.flow.is_some_and(|flow| flow.failed_start(-300.0)));

    let h = harness::build(Authority::Auto, vec![reading.clone()], plan(NOW, 1.0, 0.96));
    h.engine.boot(NOW).await.expect("boot");
    let snapshot = h.engine.snapshot(
        NOW + 1,
        &FleetView::new(vec![reading]),
        &MeterReading::default(),
    );
    assert!(
        snapshot.packs[0]
            .flow_alarm
            .as_deref()
            .is_some_and(|alarm| alarm.contains("status mapping"))
    );
    assert!(snapshot.packs[0].control_fault.is_none());
}

#[tokio::test]
async fn active_backoff_and_historical_fault_are_separate_in_the_snapshot() {
    let h = harness::build(Authority::Auto, vec![silent_pack()], plan(NOW, 1.0, 0.96));
    h.engine.boot(NOW).await.expect("boot");
    h.engine
        .override_pack("SN-A", OverrideAction::Charge { watts: 1000.0 }, 600, NOW)
        .expect("override");
    for offset in 1..=3 {
        h.engine
            .tick_once(NOW + 5 * offset)
            .await
            .expect("fault tick");
    }
    let mut quiet = silent_pack();
    quiet.historical_fault = Some("device restarted".to_owned());
    let snapshot = h.engine.snapshot(
        NOW + 21,
        &FleetView::new(vec![quiet]),
        &MeterReading::default(),
    );
    assert!(
        snapshot.packs[0]
            .control_fault
            .as_deref()
            .is_some_and(|fault| fault.contains("no AC power flowed"))
    );
    assert_eq!(
        snapshot.packs[0].last_control_fault.as_deref(),
        Some("device restarted")
    );
}
