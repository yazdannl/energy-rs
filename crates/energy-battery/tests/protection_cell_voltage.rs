// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use chrono::{DateTime, Duration, TimeZone, Utc};
use energy_battery::{CellConfig, CellReading, CellVoltageGuard, LatchReason};
use support::latch_store::MemoryLatchStore;

const PACK: &str = "CO4A000000000001";
const OTHER: &str = "CO4A000000000002";

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
}

fn config() -> CellConfig {
    CellConfig::new(3.00, 3.18, 180.0, 2000, 800)
}

#[test]
fn a_pack_that_was_never_read_is_not_latched() {
    let mut guard = CellVoltageGuard::new();
    let decision = guard.evaluate(PACK, &CellReading::missing(t0()), &config(), t0());
    assert!(!decision.changed);
    assert!(!decision.blocks_discharge());
    assert!(decision.force_charge_w().is_none());
    assert_eq!(decision.latch.reason, LatchReason::MissingReading);
    assert!(guard.latch(PACK).is_some());
}

#[test]
fn below_cutoff_blocks_discharge_and_forces_the_emergency_charge() {
    let mut guard = CellVoltageGuard::new();
    let decision = guard.evaluate(PACK, &CellReading::fresh(2.95, t0()), &config(), t0());
    assert!(decision.changed);
    assert!(decision.blocks_discharge());
    assert_eq!(decision.force_charge_w(), Some(800));
    assert!(decision.latch.forces_charge());
    assert_eq!(decision.latch.reason, LatchReason::BelowCutoff);
    assert_eq!(decision.latch.since, t0());
}

#[test]
fn between_cutoff_and_resume_only_blocks_discharge() {
    let mut guard = CellVoltageGuard::new();
    let decision = guard.evaluate(PACK, &CellReading::fresh(3.10, t0()), &config(), t0());
    assert!(decision.blocks_discharge());
    assert!(decision.force_charge_w().is_none());
    assert_eq!(decision.latch.reason, LatchReason::BelowResume);
}

#[test]
fn at_resume_the_latch_clears() {
    let mut guard = CellVoltageGuard::new();
    guard.evaluate(PACK, &CellReading::fresh(3.05, t0()), &config(), t0());
    let decision = guard.evaluate(
        PACK,
        &CellReading::fresh(3.18, t0() + Duration::minutes(1)),
        &config(),
        t0() + Duration::minutes(1),
    );
    assert!(decision.changed);
    assert!(!decision.blocks_discharge());
    assert_eq!(decision.latch.reason, LatchReason::AboveResume);
}

#[test]
fn a_stale_reading_keeps_the_previous_latch() {
    let mut guard = CellVoltageGuard::new();
    guard.evaluate(PACK, &CellReading::fresh(2.95, t0()), &config(), t0());
    let later = t0() + Duration::seconds(200);
    let decision = guard.evaluate(PACK, &CellReading::fresh(2.95, t0()), &config(), later);
    assert!(!decision.changed);
    assert!(decision.blocks_discharge());
    assert_eq!(decision.force_charge_w(), Some(800));
    assert_eq!(decision.latch.reason, LatchReason::StaleReading);
    assert_eq!(decision.latch.reason_at, later);
    assert_eq!(decision.latch.since, t0());
}

#[test]
fn a_missing_reading_keeps_the_previous_latch() {
    let mut guard = CellVoltageGuard::new();
    guard.evaluate(PACK, &CellReading::fresh(3.10, t0()), &config(), t0());
    let decision = guard.evaluate(
        PACK,
        &CellReading::missing(t0() + Duration::seconds(5)),
        &config(),
        t0() + Duration::seconds(5),
    );
    assert!(decision.blocks_discharge());
    assert!(!decision.changed);
    assert_eq!(decision.latch.reason, LatchReason::MissingReading);
}

#[test]
fn a_reading_exactly_at_the_stale_limit_is_still_evidence() {
    let mut guard = CellVoltageGuard::new();
    guard.evaluate(PACK, &CellReading::fresh(3.30, t0()), &config(), t0());
    let limit = t0() + Duration::seconds(180);
    let decision = guard.evaluate(PACK, &CellReading::fresh(3.30, t0()), &config(), limit);
    assert_eq!(decision.latch.last_fresh_at, Some(limit));
    assert_eq!(decision.latch.reason, LatchReason::AboveResume);
}

#[test]
fn the_latch_survives_a_reboot() {
    let store = MemoryLatchStore::new();
    let mut guard = CellVoltageGuard::new();
    guard.evaluate(PACK, &CellReading::fresh(2.95, t0()), &config(), t0());
    guard.save(&store, PACK).unwrap();

    let mut rebooted = CellVoltageGuard::new();
    assert!(rebooted.restore(&store, PACK).unwrap());
    let restored = rebooted.latch(PACK).unwrap();
    assert!(restored.blocks_discharge());
    assert_eq!(restored.force_charge_w, Some(800));
    assert_eq!(restored.reason, LatchReason::BelowCutoff);
    assert_eq!(restored.since, t0());

    let cleared = rebooted.evaluate(
        PACK,
        &CellReading::fresh(3.25, t0() + Duration::minutes(5)),
        &config(),
        t0() + Duration::minutes(5),
    );
    assert!(!cleared.blocks_discharge());
    assert!(!rebooted.latch(PACK).unwrap().blocks_discharge());
}

#[test]
fn restoring_an_unknown_pack_restores_nothing() {
    let store = MemoryLatchStore::new();
    let mut guard = CellVoltageGuard::new();
    assert!(!guard.restore(&store, PACK).unwrap());
    assert!(guard.latch(PACK).is_none());
}

#[test]
fn an_emergency_power_of_zero_means_the_full_ceiling() {
    let config = CellConfig::new(3.00, 3.18, 180.0, 1500, 0);
    assert_eq!(config.force_charge_w(), 1500);
    let config = CellConfig::new(3.00, 3.18, 180.0, 600, 1200);
    assert_eq!(config.force_charge_w(), 600);
}

#[test]
fn a_misconfigured_resume_below_cutoff_still_fails_safe() {
    let config = CellConfig::new(3.20, 3.00, 180.0, 1000, 500);
    let mut guard = CellVoltageGuard::new();
    let low = guard.evaluate(PACK, &CellReading::fresh(3.10, t0()), &config, t0());
    assert!(low.blocks_discharge());
    assert_eq!(low.force_charge_w(), Some(500));
    let high = guard.evaluate(
        PACK,
        &CellReading::fresh(3.25, t0() + Duration::minutes(1)),
        &config,
        t0() + Duration::minutes(1),
    );
    assert!(!high.blocks_discharge());
}

#[test]
fn saving_all_writes_every_known_pack() {
    let store = MemoryLatchStore::new();
    let mut guard = CellVoltageGuard::new();
    guard.evaluate(PACK, &CellReading::fresh(2.95, t0()), &config(), t0());
    guard.evaluate(OTHER, &CellReading::fresh(3.30, t0()), &config(), t0());
    assert_eq!(guard.save_all(&store).unwrap(), 2);
    assert!(store.latch(PACK).unwrap().blocks_discharge());
    assert!(!store.latch(OTHER).unwrap().blocks_discharge());
}
