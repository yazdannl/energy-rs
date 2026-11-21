//! A pack that really flows below its command is current-limited, not a fault.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

mod support;

use energy_core::dispatch::{
    CURTAILMENT_PAUSE_S, FleetView, MeterReading, OverrideAction, PackFlow,
};
use energy_core::safety::Authority;

use support::fakes::{NOW, pack, plan};
use support::harness;

fn limited_pack() -> energy_core::dispatch::PackReading {
    let mut reading = pack("SN-A", 12.0);
    reading.flow = Some(PackFlow {
        grid_input_w: 45,
        output_pack_w: 0,
        output_home_w: 0,
        ac_status: Some(2),
        dc_status: Some(1),
    });
    reading
}

#[tokio::test]
async fn a_current_limited_charge_has_no_hold_no_timeout_and_no_write_storm() {
    let h = harness::build(Authority::Auto, vec![limited_pack()], plan(NOW, 1.0, 0.96));
    h.engine.boot(NOW).await.expect("boot");
    h.sink.curtail(45);
    h.engine
        .override_pack("SN-A", OverrideAction::Charge { watts: 1000.0 }, 600, NOW)
        .expect("override");

    h.engine.tick_once(NOW + 5).await.expect("curtailed tick");
    assert_eq!(h.sink.writes(), vec![800.0]);
    assert_eq!(h.sink.holds(), 0, "curtailment is not a fault");
    let cause = h
        .engine
        .dispatcher()
        .faults()
        .blocker("SN-A", NOW + 10)
        .expect("a short non-fault pause");
    assert!(
        cause.contains("current-limited"),
        "unexpected cause: {cause}"
    );

    h.engine.tick_once(NOW + 20).await.expect("pause tick");
    assert_eq!(
        h.sink.writes(),
        vec![800.0],
        "no re-command during the pause"
    );
    assert_eq!(h.sink.holds(), 0);

    let snapshot = h.engine.snapshot(
        NOW + 20,
        &FleetView::new(vec![limited_pack()]),
        &MeterReading::default(),
    );
    assert!(
        snapshot.packs[0]
            .control_fault
            .as_deref()
            .is_some_and(|cause| cause.contains("current-limited"))
    );
    assert!(snapshot.packs[0].last_control_fault.is_none());

    let after_pause = NOW + CURTAILMENT_PAUSE_S + 10;
    h.engine.tick_once(after_pause).await.expect("pause expiry");
    assert_eq!(h.sink.writes().len(), 2, "the pack becomes writable again");
    assert_eq!(h.sink.holds(), 0);
}
