//! The selection latch end to end: what a tick persists, and what a restart restores.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::engine::EngineConfig;
use energy_core::safety::Authority;
use energy_ledger::selection::SelectionRow;
use support::fakes::{NOW, pack, plan};
use support::harness;

fn engine(mode: Authority, packs: Vec<energy_core::dispatch::PackReading>, plan: energy_core::plan::Plan) -> harness::Harness {
    harness::build(mode, packs, plan)
}

fn stored(h: &harness::Harness) -> Option<SelectionRow> {
    h.ledger.selection().expect("selection").get().expect("read")
}

/// Boot and run the first two ticks: the first one after a boot still holds, the second commands.
async fn drive(h: &harness::Harness, now: i64) -> i64 {
    h.engine.boot(now).await.expect("boot");
    h.engine.tick_once(now).await.expect("first tick");
    h.engine.tick_once(now + 5).await.expect("second tick");
    now + 5
}

#[tokio::test]
async fn the_tick_latches_the_pack_that_took_the_cycle() {
    let h = engine(
        Authority::Auto,
        vec![pack("SN-A", 30.0), pack("SN-B", 70.0)],
        plan(NOW, -0.7, 0.96),
    );
    let latched_at = drive(&h, NOW).await;

    let row = stored(&h).expect("a latched selection");
    assert_eq!(row.holder, "SN-B", "the fullest pack discharges");
    assert_eq!(row.direction, "discharge");
    assert_eq!(row.selected_at, latched_at);
}

#[tokio::test]
async fn the_hold_survives_further_ticks_without_rewriting_its_start() {
    let h = engine(
        Authority::Auto,
        vec![pack("SN-A", 30.0), pack("SN-B", 70.0)],
        plan(NOW, -0.7, 0.96),
    );
    let latched_at = drive(&h, NOW).await;
    h.engine.tick_once(NOW + 10).await.expect("third tick");

    let row = stored(&h).expect("a latched selection");
    assert_eq!(row.holder, "SN-B");
    assert_eq!(row.selected_at, latched_at, "the hold does not renew itself");
}

#[tokio::test]
async fn an_idle_tick_releases_the_hold() {
    let h = engine(
        Authority::Auto,
        vec![pack("SN-A", 30.0), pack("SN-B", 70.0)],
        plan(NOW, -0.7, 0.96),
    );
    drive(&h, NOW).await;
    assert!(stored(&h).is_some());

    // Both packs reach their floor, so the split has nothing to give and the fleet is idle.
    h.fleet.set(vec![pack("SN-A", 5.0), pack("SN-B", 5.0)]);
    h.engine.tick_once(NOW + 10).await.expect("idle tick");
    assert!(stored(&h).is_none(), "an idle fleet holds nothing");
}

#[tokio::test]
async fn a_dry_run_tick_latches_nothing() {
    let h = engine(
        Authority::DryRun,
        vec![pack("SN-A", 30.0), pack("SN-B", 70.0)],
        plan(NOW, -0.7, 0.96),
    );
    drive(&h, NOW).await;
    assert!(
        stored(&h).is_none(),
        "nothing reached a device, so nothing is owned"
    );
}

#[tokio::test]
async fn a_restart_restores_a_live_hold() {
    let packs = vec![pack("SN-A", 30.0), pack("SN-B", 70.0)];
    let first = engine(Authority::Auto, packs.clone(), plan(NOW, -0.7, 0.96));
    let latched_at = drive(&first, NOW).await;
    let ledger = std::sync::Arc::clone(&first.ledger);

    let second = harness::build_on(ledger, Authority::Auto, packs, plan(NOW + 5, -0.7, 0.96));
    second.engine.boot(NOW + 10).await.expect("reboot");
    let row = stored(&second).expect("the hold survived the restart");
    assert_eq!(row.holder, "SN-B");
    assert_eq!(row.selected_at, latched_at);
}

#[tokio::test]
async fn a_stale_hold_never_pins_a_pack_after_a_restart() {
    let packs = vec![pack("SN-A", 30.0), pack("SN-B", 70.0)];
    let first = engine(Authority::Auto, packs.clone(), plan(NOW, -0.7, 0.96));
    drive(&first, NOW).await;
    let ledger = std::sync::Arc::clone(&first.ledger);

    // A day later the hold is long expired.
    let later = NOW + 86_400;
    let second = harness::build_on(ledger, Authority::Auto, packs, plan(later, -0.7, 0.96));
    second.engine.boot(later).await.expect("reboot");
    assert!(
        stored(&second).is_none(),
        "an expired hold is dropped, not trusted"
    );
}

#[tokio::test]
async fn the_hold_defaults_to_thirty_minutes() {
    assert_eq!(EngineConfig::default().pack_hold_seconds, 1800);
}
