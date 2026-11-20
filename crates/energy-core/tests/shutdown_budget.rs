//! Shutdown holds: concurrent, bounded, and journalled per pack.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap
)]

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use energy_core::dispatch::PackReading;
use energy_core::engine::{Engine, EngineError, PACK_HOLD_BUDGET};
use energy_core::safety::Authority;
use energy_ledger::EventLevel;
use support::fakes::{NOW, Sent, pack, plan};
use support::harness;

const IDS: [&str; 4] = ["SN-A", "SN-B", "SN-C", "SN-D"];
const ECHO: Duration = Duration::from_secs(6);

fn packs() -> Vec<PackReading> {
    IDS.iter().map(|id| pack(id, 50.0)).collect()
}

/// Four driven packs: the second tick latches a setpoint on each one.
async fn driven() -> harness::Harness {
    let h = harness::build(Authority::Auto, packs(), plan(NOW, -4.0, 0.96));
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    h.engine.tick_once(NOW + 5).await.expect("tick");
    assert_eq!(h.engine.latch().expect("latch").len(), IDS.len());
    h
}

#[tokio::test(start_paused = true)]
async fn four_slow_holds_confirm_concurrently_within_the_shutdown_budget() {
    let h = driven().await;
    let before = h.sink.len();
    for id in IDS {
        h.sink.delay_hold(id, ECHO);
    }
    assert!(
        ECHO + Duration::from_secs(2) <= PACK_HOLD_BUDGET,
        "one pacer slot plus one echo window must fit the per-pack budget"
    );

    let start = tokio::time::Instant::now();
    h.engine
        .stop(NOW + 10, "sigterm")
        .await
        .expect("all four holds verified");
    let elapsed = start.elapsed();

    assert!(h.engine.latch().expect("latch").is_empty());
    let after: Vec<_> = h
        .sink
        .commands()
        .into_iter()
        .skip(before)
        .filter(|(_, sent)| *sent == Sent::Hold)
        .collect();
    assert_eq!(after.len(), IDS.len(), "one hold per device");
    for id in IDS {
        assert_eq!(
            after.iter().filter(|(pack_id, _)| pack_id == id).count(),
            1,
            "{id} was not held exactly once"
        );
    }
    assert!(
        elapsed < PACK_HOLD_BUDGET,
        "serial holds would take {}s; fan-out took {elapsed:?}",
        ECHO.as_secs() * u64::try_from(IDS.len()).unwrap_or(u64::MAX)
    );
    let events = h
        .ledger
        .events()
        .expect("events")
        .recent(50)
        .expect("recent");
    assert!(
        events.iter().all(|event| event.kind != "hold.unverified"),
        "every slow pack confirmed: {events:?}"
    );
    assert!(events.iter().any(|event| {
        event.kind == "engine.shutdown" && event.message.contains("4 pack(s) held")
    }));
}

#[tokio::test(start_paused = true)]
async fn an_unconfirmed_pack_is_reported_per_pack_without_delaying_the_others() {
    let h = driven().await;
    for id in IDS.iter().filter(|id| **id != "SN-D") {
        h.sink.delay_hold(id, ECHO);
    }
    h.sink.never_confirm_hold("SN-D");

    let start = tokio::time::Instant::now();
    let error = h.engine.stop(NOW + 10, "sigterm").await.expect_err("echo");
    let elapsed = start.elapsed();
    match error {
        EngineError::Holds { failed, detail } => {
            assert_eq!(failed, 1, "{detail}");
            assert!(detail.contains("SN-D"), "the stuck pack must be named");
        }
        other => panic!("expected a per-pack hold failure, got {other:?}"),
    }
    assert!(
        elapsed <= PACK_HOLD_BUDGET + Duration::from_secs(1),
        "a stuck echo must not outlive the budget: {elapsed:?}"
    );

    let latch = h.engine.latch().expect("latch");
    assert_eq!(latch.len(), 1, "the verified holds cleared their latch");
    assert!(latch.get("SN-D").is_some(), "the stuck pack stays owned");

    let events = h
        .ledger
        .events()
        .expect("events")
        .recent(50)
        .expect("recent");
    let unverified: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "hold.unverified")
        .collect();
    assert_eq!(unverified.len(), 1, "one journal line, for the stuck pack");
    assert!(unverified[0].message.contains("SN-D"));
    assert_eq!(unverified[0].level, EventLevel::Warn);

    let dispatch = h
        .ledger
        .dispatch()
        .expect("dispatch")
        .range(NOW, NOW + 11)
        .expect("range");
    let failed = dispatch
        .iter()
        .filter(|entry| {
            entry
                .error
                .as_deref()
                .is_some_and(|error| error.contains("budget"))
        })
        .count();
    assert_eq!(failed, 1, "the dispatch row also names the unverified hold");
}

#[tokio::test(start_paused = true)]
async fn shutdown_fences_the_generation_before_any_hold() {
    let h = driven().await;
    let before = h.engine.generation();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::downgrade(&h.engine);
    let recorded = Arc::clone(&seen);
    h.sink.observe_holds(Arc::new(move |_| {
        let live: Arc<Engine> = engine.upgrade().expect("engine alive");
        recorded.lock().expect("recorded").push(live.generation());
    }));

    h.engine.stop(NOW + 10, "sigterm").await.expect("holds");
    let seen = seen.lock().expect("seen");
    assert_eq!(seen.len(), IDS.len());
    assert!(
        seen.iter().all(|generation| *generation > before),
        "holds ran before the fence: before {before}, seen {seen:?}"
    );
}
