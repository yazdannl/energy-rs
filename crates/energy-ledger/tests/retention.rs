//! Retention: the 30-day / 1-year boundaries, count trims, current-day protection and idempotence.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use energy_ledger::{DispatchEntry, Plan, PlanSlot, RetentionPolicy, Tier};
use support::{TempDatabase, register_pack, sample, write};

/// 2026-06-15T10:00:00Z, local noon in Amsterdam.
const NOW: i64 = 1_781_517_600;
/// 2026-06-14T22:00:00Z, local midnight.
const DAY_START: i64 = NOW - 43_200;
const DAY: i64 = 86_400;

fn one_minute_count(ledger: &energy_ledger::Ledger, pack: i64) -> usize {
    ledger
        .measurements()
        .unwrap()
        .range(Tier::OneMinute, pack, 0, NOW + DAY)
        .unwrap()
        .len()
}

fn five_minute_count(ledger: &energy_ledger::Ledger, pack: i64) -> usize {
    ledger
        .measurements()
        .unwrap()
        .range(Tier::FiveMinute, pack, 0, NOW + DAY)
        .unwrap()
        .len()
}

#[test]
fn one_minute_rows_expire_after_thirty_days_but_not_at_the_boundary() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, NOW - 31 * DAY, 100.0, Some(50.0)),
            sample(pack, NOW - 30 * DAY, 100.0, Some(50.0)),
            sample(pack, NOW - 29 * DAY, 100.0, Some(50.0)),
        ],
    );
    let report = ledger.run_retention(NOW).unwrap();
    assert_eq!(report.measurements_1m, 1);
    assert_eq!(one_minute_count(&ledger, pack), 2);
}

#[test]
fn five_minute_rows_expire_after_a_year_but_not_at_the_boundary() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, NOW - 366 * DAY, 100.0, Some(50.0)),
            sample(pack, NOW - 365 * DAY, 100.0, Some(50.0)),
            sample(pack, NOW - 364 * DAY, 100.0, Some(50.0)),
        ],
    );
    let measurements = ledger.measurements().unwrap();
    for offset in [366, 365, 364] {
        let ts = NOW - offset * DAY;
        measurements.rollup(ts, ts + 300).unwrap();
    }
    drop(measurements);
    let before = five_minute_count(&ledger, pack);
    let report = ledger.run_retention(NOW).unwrap();
    assert_eq!(report.measurements_5m, 1);
    assert_eq!(five_minute_count(&ledger, pack), before - 1);
}

#[test]
fn a_trim_never_deletes_rows_inside_the_current_local_day() {
    let db = TempDatabase::new();
    let ledger = db.ledger_with(RetentionPolicy::new(0, 0, 200, 20_000));
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, DAY_START - 1, 100.0, Some(50.0)),
            sample(pack, DAY_START, 100.0, Some(50.0)),
            sample(pack, NOW, 100.0, Some(50.0)),
        ],
    );
    let report = ledger.run_retention(NOW).unwrap();
    assert_eq!(report.measurements_1m, 1);
    assert_eq!(one_minute_count(&ledger, pack), 2);
}

fn insert_plan(ledger: &energy_ledger::Ledger, created_at: i64) -> i64 {
    let mut first = PlanSlot::at(0, created_at);
    first.battery_kw = 1.0;
    let mut second = PlanSlot::at(1, created_at + 900);
    second.battery_kw = -1.0;
    ledger
        .plans()
        .unwrap()
        .insert(
            &Plan::new(created_at, "dry_run", created_at, created_at + 1_800, "{}"),
            &[first, second],
        )
        .unwrap()
        .id
}

#[test]
fn plans_are_trimmed_to_the_newest_two_hundred_with_their_slots() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let base = NOW - 10 * DAY;
    for offset in 0..205 {
        insert_plan(&ledger, base + offset);
    }
    let report = ledger.run_retention(NOW).unwrap();
    assert_eq!(report.plans, 5);
    assert_eq!(report.plan_slots, 10);
    let plans = ledger.plans().unwrap().range(0, NOW + DAY).unwrap();
    assert_eq!(plans.len(), 200);
    assert_eq!(plans.last().unwrap().created_at, base + 204);
    let again = ledger.run_retention(NOW).unwrap();
    assert_eq!(again.total(), 0);
}

#[test]
fn a_plan_from_today_survives_the_count_trim() {
    let db = TempDatabase::new();
    let ledger = db.ledger_with(RetentionPolicy::new(30, 365, 2, 20_000));
    let base = NOW - 10 * DAY;
    for offset in 0..3 {
        insert_plan(&ledger, base + offset);
    }
    let today = insert_plan(&ledger, NOW);
    let report = ledger.run_retention(NOW).unwrap();
    assert_eq!(report.plans, 2);
    let plans = ledger.plans().unwrap().range(0, NOW + DAY).unwrap();
    assert_eq!(plans.len(), 2);
    assert_eq!(
        ledger.plans().unwrap().current().unwrap().unwrap().id,
        today
    );
}

#[test]
fn dispatch_is_trimmed_by_count_and_protects_today() {
    let db = TempDatabase::new();
    let ledger = db.ledger_with(RetentionPolicy::new(30, 365, 200, 3));
    let pack = register_pack(&ledger, "ABC123");
    let base = NOW - 10 * DAY;
    for offset in 0..5 {
        let entry = DispatchEntry::new(base + offset, Some(pack), 500.0, 500.0, "plan", "auto")
            .verified(500.0);
        ledger.dispatch().unwrap().insert(&entry).unwrap();
    }
    let today = DispatchEntry::new(NOW, Some(pack), 0.0, 0.0, "hold", "dry_run");
    ledger.dispatch().unwrap().insert(&today).unwrap();
    let report = ledger.run_retention(NOW).unwrap();
    assert_eq!(report.dispatch_log, 3);
    let rows = ledger.dispatch().unwrap().range(0, NOW + DAY).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows.last().unwrap().ts, NOW);
    let again = ledger.run_retention(NOW).unwrap();
    assert_eq!(again.dispatch_log, 0);
}

#[test]
fn sessions_and_daily_savings_are_kept_forever() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    let mut day = energy_ledger::SavingsDaily::new("2020-01-01", 1.0, 2.0);
    day.complete = true;
    ledger.savings().unwrap().upsert(&day).unwrap();
    write(
        &ledger,
        &[
            sample(pack, 0, 0.0, Some(50.0)),
            sample(pack, 60, 1_000.0, Some(51.0)),
            sample(pack, 120, 1_000.0, Some(52.0)),
            sample(pack, 180, 0.0, Some(52.5)),
        ],
    );
    ledger
        .sessions()
        .unwrap()
        .extract(
            pack,
            Tier::OneMinute,
            0,
            240,
            &energy_ledger::SessionRules::default(),
        )
        .unwrap();
    ledger.run_retention(NOW).unwrap();
    assert_eq!(ledger.sessions().unwrap().range(0, 300).unwrap().len(), 1);
    assert_eq!(
        ledger
            .savings()
            .unwrap()
            .get("2020-01-01")
            .unwrap()
            .unwrap()
            .saving_eur,
        1.0
    );
}

#[test]
fn events_are_trimmed_to_the_newest_kept_rows_but_today_is_never_touched() {
    let db = TempDatabase::new();
    let ledger = db.ledger_with(RetentionPolicy::default().with_events_kept(5));
    {
        let journal = ledger.events().unwrap();
        for index in 0..5 {
            journal
                .insert(&energy_ledger::Event::new(
                    NOW - DAY - index,
                    energy_ledger::EventLevel::Info,
                    "test.old",
                    "old",
                ))
                .unwrap();
        }
        for index in 0..3 {
            journal
                .insert(&energy_ledger::Event::new(
                    NOW - index,
                    energy_ledger::EventLevel::Info,
                    "test.today",
                    "today",
                ))
                .unwrap();
        }
    }
    let report = ledger.run_retention(NOW).unwrap();
    assert_eq!(report.events, 3, "eight rows, the newest five kept");
    let remaining = ledger.events().unwrap().range(0, NOW + 1).unwrap();
    assert_eq!(remaining.len(), 5, "two old plus every today row");
    assert!(remaining.iter().any(|event| event.kind == "test.today"));
}
