//! Authorization-level probes only (S333-S337), not real-path enforcement.
//! The crate is unwired: no engine entry point, hardware neutralisation, settings mutation or
//! websocket forwarding is invoked. Coverage of those paths is DEFERRED in M6-AMENDMENT.md.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod m4_support;
mod m6_support;

use energy_decide::authority::{Caller, EntryPoint, WriteClass};
use energy_decide::dispatch::{DispatchState, Dispatcher, ProceedSafety, SlotRequest};
use m4_support::{FakeIo, ImmediateClock};
use m6_support::{auto_fixture, latched_fixture};

fn dispatch_pair(safety: bool, at_ms: i64) {
    let request = if safety {
        SlotRequest::safety("P", 0.0, at_ms, "paired safety")
    } else {
        SlotRequest::economic("P", -800.0, at_ms, "paired economic")
    };

    let latched = latched_fixture(at_ms);
    let latched_io = FakeIo::default();
    let latched_dut = Dispatcher::new(
        latched_io.clone(),
        ProceedSafety,
        latched.authority,
        ImmediateClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let rows = latched_dut.dispatch_slot(&mut state, std::slice::from_ref(&request), at_ms);
    assert!(!rows[0].written, "latched dispatch must write nothing");
    assert_eq!(latched_io.write_count(), 0);

    let auto = auto_fixture(at_ms);
    let auto_io = FakeIo::default();
    let auto_dut = Dispatcher::new(
        auto_io.clone(),
        ProceedSafety,
        auto.authority,
        ImmediateClock::default(),
        20_000,
    );
    let mut auto_state = DispatchState::default();
    let auto_rows = auto_dut.dispatch_slot(&mut auto_state, std::slice::from_ref(&request), at_ms);
    assert!(auto_rows[0].written, "auto dispatch must proceed");
    assert_eq!(auto_io.write_count(), 1);
}

#[test]
fn s333_engine_dispatch_latched_refused_auto_proceeds() {
    dispatch_pair(false, 40_000);
}

#[test]
fn s334_safety_traffic_latched_refused_auto_proceeds() {
    dispatch_pair(true, 41_000);
}

#[test]
fn s335_boot_shutdown_neutralisation_latched_refused_auto_proceeds() {
    let latched = latched_fixture(42_000);
    assert!(
        !latched
            .authority
            .authorize_boot_neutralisation(42_000, "boot-42")
            .authorized()
    );
    assert!(
        !latched
            .authority
            .authorize_shutdown_neutralisation(42_000, "shutdown-42")
            .authorized()
    );

    let auto = auto_fixture(42_000);
    assert!(
        auto.authority
            .authorize_boot_neutralisation(42_000, "boot-42")
            .authorized()
    );
    assert!(
        auto.authority
            .authorize_shutdown_neutralisation(42_000, "shutdown-42")
            .authorized()
    );
}

#[test]
fn s336_settings_mode_control_plane_latched_refused_auto_proceeds() {
    let latched = latched_fixture(43_000);
    assert!(
        !latched
            .authority
            .authorize_settings_write("api", 43_000, "settings mode")
            .authorized()
    );
    let auto = auto_fixture(43_000);
    assert!(
        auto.authority
            .authorize_settings_write("api", 43_000, "settings mode")
            .authorized()
    );
}

#[test]
fn s337_generic_ws_passthrough_latched_refused_auto_proceeds() {
    let caller = |at_ms| {
        Caller::system(
            EntryPoint::GenericWsPassthrough,
            at_ms,
            "raw generic websocket call",
        )
    };
    let latched = latched_fixture(44_000);
    assert!(
        !latched
            .authority
            .authorize_ws_passthrough(caller(44_000), WriteClass::DeviceEconomic)
            .authorized()
    );
    let auto = auto_fixture(44_000);
    assert!(
        auto.authority
            .authorize_ws_passthrough(caller(44_000), WriteClass::DeviceEconomic)
            .authorized()
    );
}
