//! The failure boundary: durable ownership, failed ticks, orphans and bookkeeping failures.
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

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};

use energy_core::engine::{EngineError, LiveLatch};
use energy_core::safety::Authority;
use energy_ledger::Ledger;
use support::fakes::{self, NOW, Sent, pack, plan};
use support::harness;

/// A two-pack `auto` engine that already drove both packs (so both are latched).
///
/// The plan asks for both packs' full 800 W: ownership must not depend on the split policy, so the
/// fixture drives two packs because the request needs two, not because the target was halved.
async fn driven() -> harness::Harness {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0), pack("SN-B", 50.0)],
        plan(NOW, -1.6, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    h.engine.tick_once(NOW + 5).await.expect("tick");
    assert_eq!(h.engine.latch().expect("latch").len(), 2);
    h
}

/// The setpoint is durable before the transport call: a crash in between still knows the pack.
#[tokio::test]
async fn the_write_intent_is_durable_before_the_transport_call() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let ledger = Arc::clone(&h.ledger);
    let observed = Arc::clone(&seen);
    h.sink.observe(Arc::new(move |pack_id: &str| {
        let latch = LiveLatch::load(&ledger).expect("latch reload");
        observed
            .lock()
            .expect("observed")
            .push(latch.get(pack_id).is_some());
    }));
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    h.engine.tick_once(NOW + 5).await.expect("tick");

    assert_eq!(
        *seen.lock().expect("observed"),
        vec![true],
        "the pack must be latched before the command leaves"
    );
    assert_eq!(LiveLatch::load(&h.ledger).expect("reload").len(), 1);
}

/// A tick that cannot read never marks progress and holds what the process owns.
#[tokio::test]
async fn a_failing_tick_holds_what_we_own_and_stops_marking_progress() {
    let h = driven().await;
    h.sink.unverified_holds(true);
    let before = h.sink.holds();

    h.fleet.fail(true);
    for step in 1..=3 {
        let at = NOW + 5 * step;
        let error = h.engine.tick_once(at).await.expect_err("the read fails");
        h.engine.tick_failed(at, &error).await;
    }
    assert!(
        h.sink.holds() >= before + 3,
        "a failed tick must hold the packs it owns"
    );
    assert_eq!(
        h.engine.latch().expect("latch").len(),
        2,
        "an unverified hold keeps ownership"
    );
    assert!(
        h.engine.supervise(NOW + 21).await.expect("guard"),
        "repeated failed ticks must still reach the watchdog"
    );
}

/// A pack that vanished from the read is hardware we drove: it is held, not forgotten.
#[tokio::test]
async fn a_pack_that_vanished_is_held_and_keeps_its_latch() {
    let h = driven().await;
    h.sink.unverified_holds(true);
    let mark = h.sink.len();
    h.fleet.set(vec![pack("SN-A", 50.0)]);
    let outcome = h.engine.tick_once(NOW + 5).await.expect("tick");

    assert!(outcome.report("SN-B").is_none());
    let after: Vec<(String, Sent)> = h.sink.commands().into_iter().skip(mark).collect();
    assert!(
        after
            .iter()
            .any(|(pack_id, sent)| pack_id == "SN-B" && *sent == Sent::Hold),
        "the vanished pack was never held: {after:?}"
    );
    assert!(
        h.engine.latch().expect("latch").get("SN-B").is_some(),
        "ownership must survive until the device confirms"
    );
}

/// A pack that reports itself unavailable is held too; availability is not a reason to keep a setpoint.
#[tokio::test]
async fn an_unavailable_pack_is_held_and_keeps_its_latch() {
    let h = driven().await;
    h.sink.unverified_holds(true);
    let mut gone = pack("SN-B", 50.0);
    gone.available = false;
    let mark = h.sink.len();
    h.fleet.set(vec![pack("SN-A", 50.0), gone]);
    let outcome = h.engine.tick_once(NOW + 5).await.expect("tick");

    assert!(!outcome.report("SN-B").expect("report").written);
    let after: Vec<(String, Sent)> = h.sink.commands().into_iter().skip(mark).collect();
    assert!(
        after
            .iter()
            .any(|(pack_id, sent)| pack_id == "SN-B" && *sent == Sent::Hold),
        "the unavailable pack was never held: {after:?}"
    );
    assert!(h.engine.latch().expect("latch").get("SN-B").is_some());
}

/// Break the ledger for every later caller by panicking while its connection mutex is held.
fn break_the_ledger(ledger: &Arc<Ledger>) {
    let ledger = Arc::clone(ledger);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(move || {
        let _guard = ledger.settings().expect("settings guard");
        panic!("poison the ledger connection");
    }));
    std::panic::set_hook(previous);
    assert!(outcome.is_err(), "the ledger must be poisoned");
}

/// One pack's bookkeeping failure must not stop the holds of the rest of the fleet.
#[tokio::test]
async fn a_bookkeeping_failure_still_holds_every_target() {
    let h = driven().await;
    let before = h.sink.holds();
    break_the_ledger(&h.ledger);

    let error = h
        .engine
        .stop(NOW + 10, "sigterm")
        .await
        .expect_err("the hold bookkeeping fails");
    assert!(
        matches!(error, EngineError::Holds { failed: 2, .. }),
        "{error:?}"
    );
    assert_eq!(
        h.sink.holds(),
        before + 2,
        "every target must be attempted, not just the first"
    );
    assert_eq!(
        h.engine.latch().expect("latch").len(),
        2,
        "a failed hold must not drop ownership"
    );
}

/// A reopen of the same in-memory ledger is not involved: the fixture check keeps clippy honest.
#[tokio::test]
async fn the_fixture_ledger_starts_with_one_row_per_pack() {
    let ledger = fakes::ledger_with(&["SN-A", "SN-B"]);
    assert_eq!(ledger.packs().expect("packs").all().expect("rows").len(), 2);
}

/// A pack that keeps reporting something else than it was told is a control fault: it is held and
/// left alone, instead of being commanded all day by a loop that cannot see the disagreement.
#[tokio::test]
async fn a_pack_that_ignores_its_command_is_held_and_backed_off() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    // From here the pack reports 0 W while being told to discharge, tick after tick.
    for offset in 0..4 {
        h.engine
            .tick_once(NOW + 5 * (offset + 1))
            .await
            .expect("tick");
    }
    let events = h
        .ledger
        .events()
        .expect("events")
        .range(NOW, NOW + 60)
        .expect("range");
    assert!(
        events.iter().any(|event| event.kind == "dispatch.readback"),
        "the disagreement must be reported: {:?}",
        events
            .iter()
            .map(|event| event.kind.clone())
            .collect::<Vec<_>>()
    );
    // The next tick leaves the pack alone: it is inside the fault backoff the readback armed.
    let before = h.sink.len();
    h.engine.tick_once(NOW + 21).await.expect("backoff tick");
    let after: Vec<(String, Sent)> = h.sink.commands().into_iter().skip(before).collect();
    assert!(
        !after
            .iter()
            .any(|(pack_id, sent)| pack_id == "SN-A" && *sent != Sent::Hold),
        "a pack that ignores its command is not commanded again: {after:?}"
    );
}
