//! Shutdown holds: concurrent, bounded, and journalled per pack.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assert_is_empty,
    clippy::cast_possible_wrap
)]

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use energy_battery::{ECHO_CONFIRM_WINDOW, MIN_WRITE_SPACING};
use energy_core::dispatch::{PackReading, SinkError};
use energy_core::engine::{Engine, PACK_HOLD_BUDGET};
use energy_core::safety::Authority;
use energy_ledger::EventLevel;
use support::fakes::{NOW, Sent, pack, plan};
use support::harness;

const IDS: [&str; 4] = ["SN-A", "SN-B", "SN-C", "SN-D"];
const SLOW_ECHO: Duration = ECHO_CONFIRM_WINDOW;

#[test]
fn the_hold_budget_tracks_the_echo_window() {
    assert_eq!(
        PACK_HOLD_BUDGET,
        MIN_WRITE_SPACING + ECHO_CONFIRM_WINDOW + Duration::from_secs(2)
    );
}

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
        h.sink.delay_hold(id, SLOW_ECHO);
    }
    assert!(
        SLOW_ECHO + Duration::from_secs(2) <= PACK_HOLD_BUDGET,
        "one pacer slot plus one echo window must fit the per-pack budget"
    );

    let start = tokio::time::Instant::now();
    let report = h
        .engine
        .stop(NOW + 10, "sigterm")
        .await
        .expect("all four holds verified");
    let elapsed = start.elapsed();
    assert_eq!(report.verified().len(), IDS.len());
    assert!(report.unverified().is_empty());
    assert!(report.fenced().is_empty());

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
        SLOW_ECHO.as_secs() * u64::try_from(IDS.len()).unwrap_or(u64::MAX)
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
        event.kind == "engine.shutdown"
            && event.message.contains("4 verified")
            && event.message.contains("0 unverified")
    }));
}

#[tokio::test(start_paused = true)]
async fn an_unconfirmed_pack_is_reported_per_pack_without_delaying_the_others() {
    let h = driven().await;
    for id in IDS.iter().filter(|id| **id != "SN-D") {
        h.sink.delay_hold(id, SLOW_ECHO);
    }
    h.sink.never_confirm_hold("SN-D");

    let start = tokio::time::Instant::now();
    let report = h.engine.stop(NOW + 10, "sigterm").await.expect("report");
    let elapsed = start.elapsed();
    assert_eq!(report.verified().len(), 3);
    assert_eq!(report.unverified(), &[String::from("SN-D")]);
    assert!(report.fenced().is_empty());
    assert!(report.summary().contains("SN-D"));
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
    assert!(events.iter().any(|event| {
        event.kind == "engine.shutdown" && event.message.contains("1 unverified (SN-D)")
    }));

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
async fn an_unreachable_hold_is_journalled_distinctly_from_a_refused_hold() {
    let h = driven().await;
    h.sink.fail_hold(
        "SN-A",
        SinkError::DeviceUnreachable {
            pack_id: "SN-A".to_owned(),
            reason: "device report request failed".to_owned(),
        },
    );
    h.sink.fail_hold(
        "SN-B",
        SinkError::Device {
            pack_id: "SN-B".to_owned(),
            reason: "device refused the hold".to_owned(),
        },
    );
    let report = h.engine.stop(NOW + 10, "sigterm").await.expect("report");
    assert_eq!(report.unverified().len(), 2);
    let rows = h
        .ledger
        .dispatch()
        .expect("dispatch")
        .range(NOW, NOW + 11)
        .expect("rows");
    let unreachable = rows
        .iter()
        .find(|r| {
            r.error
                .as_deref()
                .is_some_and(|e| e.contains("SN-A") && e.contains("hold failed"))
        })
        .expect("unreachable hold row");
    assert_eq!(unreachable.reason, "hold_unverifiable_device_unreachable");
    assert_eq!(unreachable.applied_w_reason(), Some("read_failed"));
    assert!(
        unreachable
            .error
            .as_deref()
            .is_some_and(|e| e.contains("request failed"))
    );
    let refused = rows
        .iter()
        .find(|r| {
            r.error
                .as_deref()
                .is_some_and(|e| e.contains("SN-B") && e.contains("hold failed"))
        })
        .expect("refused hold row");
    assert_eq!(refused.reason, "shutdown_hold");
    assert_eq!(refused.applied_w_reason(), None);
    assert!(
        refused
            .error
            .as_deref()
            .is_some_and(|e| e.contains("device refused"))
    );
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

#[tokio::test(start_paused = true)]
async fn a_fenced_hold_is_reported_separately_from_verified_and_unverified() {
    let h = driven().await;
    h.sink.fence_hold("SN-C");

    let report = h.engine.stop(NOW + 10, "sigterm").await.expect("holds");
    assert_eq!(report.verified().len(), 3);
    assert_eq!(report.fenced(), &[String::from("SN-C")]);
    assert!(report.unverified().is_empty());
    assert!(report.summary().contains("1 fenced (SN-C)"));
    assert!(
        h.engine.latch().expect("latch").get("SN-C").is_some(),
        "a fenced command owns the pack until a device confirms"
    );
}

#[tokio::test(start_paused = true)]
async fn a_short_hold_budget_reports_every_pack_unverified_within_the_budget() {
    let h = driven().await;
    for id in IDS {
        h.sink.delay_hold(id, SLOW_ECHO);
    }
    let budget = Duration::from_secs(2);

    let start = tokio::time::Instant::now();
    let report = h
        .engine
        .stop_within(NOW + 10, "deadline slice", budget)
        .await
        .expect("bounded holds");
    let elapsed = start.elapsed();
    assert!(report.verified().is_empty());
    assert_eq!(report.unverified().len(), IDS.len());
    assert!(report.fenced().is_empty());
    assert!(elapsed <= budget + Duration::from_secs(1), "{elapsed:?}");
    assert_eq!(h.engine.latch().expect("latch").len(), IDS.len());
}
