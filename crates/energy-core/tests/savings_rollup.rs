//! The savings rollup: closed days, today's provisional value, catch-up, DST and idempotency.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use std::sync::Arc;

use energy_core::savings::{DayAccounting, DayReading, SavingsRollup, SettlementSeed, account};
use energy_ledger::{Ledger, SavingsDaily};
use support::savings::{self, ScriptedSource};

/// A complete row so the rollup's resume point is the day after it.
fn complete_row(ledger: &Ledger, day: &str) {
    let mut row = SavingsDaily::new(day, 0.1, 0.2);
    row.complete = true;
    row.coverage_hours = 24.0;
    ledger
        .savings()
        .expect("savings")
        .upsert(&row)
        .expect("row");
}

/// A sunny day with a constant house load and no battery.
fn sunny(day: &str, seed: SettlementSeed) -> DayReading {
    savings::reading(
        day,
        |_| 0.20,
        |_| 0.05,
        |index| {
            let pv = if (40..50).contains(&index) { 0.3 } else { 0.0 };
            (pv, 0.1, 0.0, 0.0)
        },
        seed,
    )
}

fn rollup(ledger: &Arc<Ledger>, source: Arc<ScriptedSource>, today: &str) -> SavingsRollup {
    SavingsRollup::new(Arc::clone(ledger), source, savings::settlement(today))
}

fn accounting_of(day: &str, seed: SettlementSeed) -> DayAccounting {
    account(&sunny(day, seed), &savings::settlement(day))
}

/// A recomputation over the same measurements stores byte-identical rows; once yesterday is
/// complete the next pass only refreshes today.
#[tokio::test]
async fn a_recompute_is_idempotent_and_only_todays_row_is_refreshed() {
    let ledger = savings::ledger();
    complete_row(&ledger, "2027-06-07");
    let source = ScriptedSource::new(vec![
        sunny("2027-06-08", savings::unseeded()),
        sunny("2027-06-09", savings::unseeded()),
    ]);
    let rollup = rollup(&ledger, Arc::clone(&source), "2027-06-09");
    let first = rollup
        .refresh(savings::noon("2027-06-09"))
        .await
        .expect("pass");
    assert_eq!(first.days, ["2027-06-08", "2027-06-09"]);
    assert_eq!(first.complete, 1);
    assert!(first.provisional);

    let before = ledger
        .savings()
        .expect("savings")
        .range("2027-06-08", "2027-06-09")
        .expect("rows");
    let reads = source.reads();
    let second = rollup
        .refresh(savings::noon("2027-06-09"))
        .await
        .expect("pass");
    assert_eq!(second.days, ["2027-06-09"]);
    assert_eq!(source.reads(), reads + 1);
    let after = ledger
        .savings()
        .expect("savings")
        .range("2027-06-08", "2027-06-09")
        .expect("rows");
    assert_eq!(before, after);
}

/// Three missed days (the add-on was down) are all written, in day order, before today's row.
#[tokio::test]
async fn catch_up_after_three_missed_days() {
    let ledger = savings::ledger();
    complete_row(&ledger, "2027-06-05");
    let source = ScriptedSource::new(vec![
        sunny("2027-06-06", savings::unseeded()),
        sunny("2027-06-07", savings::unseeded()),
        sunny("2027-06-08", savings::unseeded()),
        sunny("2027-06-09", savings::unseeded()),
    ]);
    let rollup = rollup(&ledger, Arc::clone(&source), "2027-06-09");
    let report = rollup
        .refresh(savings::noon("2027-06-09"))
        .await
        .expect("pass");
    assert_eq!(
        report.days,
        ["2027-06-06", "2027-06-07", "2027-06-08", "2027-06-09"]
    );
    assert_eq!(report.complete, 3);
    assert_eq!(source.reads(), 4);
    let days = ledger
        .savings()
        .expect("savings")
        .range("2027-06-06", "2027-06-08")
        .expect("rows");
    assert_eq!(days.len(), 3);
    assert!(days.iter().all(|day| day.complete && day.has_money()));
}

/// A day that cannot be read is journalled and skipped; the rest of the pass still runs, and a
/// later pass writes it once it is available.
#[tokio::test]
async fn a_failed_day_does_not_stop_the_pass() {
    let ledger = savings::ledger();
    complete_row(&ledger, "2027-06-07");
    let source = ScriptedSource::new(vec![
        sunny("2027-06-08", savings::unseeded()),
        sunny("2027-06-09", savings::unseeded()),
    ]);
    source.drop_day("2027-06-09");
    let rollup = rollup(&ledger, Arc::clone(&source), "2027-06-09");
    let report = rollup
        .refresh(savings::noon("2027-06-09"))
        .await
        .expect("pass");
    assert_eq!(report.days, ["2027-06-08"]);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].starts_with("2027-06-09"));
    assert!(
        ledger
            .savings()
            .expect("savings")
            .get("2027-06-09")
            .expect("read")
            .is_none()
    );

    source.add(sunny("2027-06-09", savings::unseeded()));
    let report = rollup
        .refresh(savings::noon("2027-06-09"))
        .await
        .expect("pass");
    assert_eq!(report.days, ["2027-06-09"]);
    assert!(report.provisional);
    assert_eq!(report.failures, Vec::<String>::new());
}

/// The newest closed day is rewritten when it was stored incomplete: late statistics can still fill
/// the day, and a day is never trusted after a gap.
#[tokio::test]
async fn an_incomplete_newest_day_is_retried() {
    let ledger = savings::ledger();
    complete_row(&ledger, "2027-06-07");
    ledger
        .savings()
        .expect("savings")
        .upsert(&SavingsDaily::incomplete("2027-06-08"))
        .expect("row");
    let source = ScriptedSource::new(vec![
        sunny("2027-06-08", savings::unseeded()),
        sunny("2027-06-09", savings::unseeded()),
    ]);
    let rollup = rollup(&ledger, Arc::clone(&source), "2027-06-09");
    let report = rollup
        .refresh(savings::noon("2027-06-09"))
        .await
        .expect("pass");
    assert_eq!(report.days, ["2027-06-08", "2027-06-09"]);
    let retried = ledger
        .savings()
        .expect("savings")
        .get("2027-06-08")
        .expect("read")
        .expect("row");
    assert!(retried.complete);
    assert!(retried.saving_eur.is_some());
}

/// The 25-hour autumn day is 100 measured slots and covers 25 hours; the 23-hour spring day is 92.
#[test]
fn dst_days_keep_their_real_length() {
    let autumn = accounting_of("2026-10-25", savings::seeded(2026, 20.0, 0.0));
    assert!(autumn.complete);
    assert_eq!(autumn.day, "2026-10-25");
    assert!((autumn.coverage_hours - 25.0).abs() < 1e-12);
    assert!((autumn.pv_kwh - 3.0).abs() < 1e-9);
    assert!((autumn.import_kwh - 9.0).abs() < 1e-9);
    // 2 kWh of surplus export is netted 1:1 inside the seed's 20 kWh budget.
    assert!((autumn.export_kwh - 2.0).abs() < 1e-9);
    assert!((autumn.actual_eur - (9.0 * 0.20 - 2.0 * 0.20)).abs() < 1e-9);

    let spring = accounting_of("2026-03-29", savings::seeded(2026, 20.0, 0.0));
    assert!(spring.complete);
    assert!((spring.coverage_hours - 23.0).abs() < 1e-12);
    assert!((spring.import_kwh - 8.2).abs() < 1e-9);
}
