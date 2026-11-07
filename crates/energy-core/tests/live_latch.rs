//! The live-write latch: what survives a restart, and what may clear it.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assert_is_empty,
    clippy::manual_assert_eq,
    clippy::assertions_on_constants,
    clippy::cast_possible_wrap
)]

mod support;

use std::sync::Arc;

use energy_core::engine::LiveLatch;
use energy_core::safety::Authority;
use support::fakes::{self, NOW, pack, plan};
use support::harness;

/// A real setpoint latches, survives a restart and only a verified hold clears it.
#[tokio::test]
async fn the_latch_survives_a_restart_and_clears_only_on_a_verified_hold() {
    let ledger = fakes::ledger_with(&["SN-A"]);
    let first = harness::build_on(
        Arc::clone(&ledger),
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    first.engine.boot(NOW).await.expect("boot");
    first.engine.tick_once(NOW).await.expect("first tick");
    first.engine.tick_once(NOW + 5).await.expect("tick");
    assert_eq!(first.engine.latch().expect("latch").len(), 1);
    assert_eq!(LiveLatch::load(&ledger).expect("reload").len(), 1);

    let second = harness::build_on(
        Arc::clone(&ledger),
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW + 5, -0.8, 0.96),
    );
    second.sink.unverified_holds(true);
    second.engine.boot(NOW + 10).await.expect("boot");
    assert_eq!(
        second.sink.holds(),
        1,
        "boot must neutralise the latched pack"
    );
    assert_eq!(
        second.engine.latch().expect("latch").len(),
        1,
        "an unverified hold must not clear the latch"
    );
    assert_eq!(LiveLatch::load(&ledger).expect("reload").len(), 1);

    let third = harness::build_on(
        Arc::clone(&ledger),
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW + 10, -0.8, 0.96),
    );
    third.engine.boot(NOW + 20).await.expect("boot");
    assert_eq!(third.sink.holds(), 1);
    assert!(third.engine.latch().expect("latch").is_empty());
    assert!(LiveLatch::load(&ledger).expect("reload").is_empty());
}

/// Boot writes nothing on hardware it never drove, and the latch stays empty.
#[tokio::test]
async fn boot_never_touches_a_device_it_did_not_drive() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0), pack("SN-B", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    assert!(
        h.sink.commands().is_empty(),
        "boot held a device with no latch: {:?}",
        h.sink.commands()
    );
    assert!(LiveLatch::load(&h.ledger).expect("latch").is_empty());
}

/// A blocked or shadowed tick never latches: only a real write does.
#[tokio::test]
async fn only_a_real_write_latches() {
    let h = harness::build(
        Authority::DryRun,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    h.engine.tick_once(NOW + 5).await.expect("tick");
    assert!(h.sink.commands().is_empty());
    assert!(h.engine.latch().expect("latch").is_empty());
    assert!(LiveLatch::load(&h.ledger).expect("latch").is_empty());
}
