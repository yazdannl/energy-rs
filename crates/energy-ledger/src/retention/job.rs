//! The trim job: age-based for measurements, count-based for plans and dispatch, never touching
//! the current local day.
//!
//! Everything runs in one transaction, so a reader never sees plans deleted before their slots.
//! Deletion cut-offs are `now - days` clamped to before the start of the current `Europe/Amsterdam`
//! day; count trims keep the newest N rows plus every row of today, so "daily summaries forever"
//! and a stable UI are not broken by a burst of plans after midnight.

use chrono::{TimeZone, Utc};
use chrono_tz::Europe::Amsterdam;
use rusqlite::{Connection, params};

use crate::error::LedgerError;
use crate::retention::{RetentionPolicy, RetentionReport};

const DAY_S: i64 = 86_400;

/// Delete rows that fall outside `policy`, as seen at `now` (UTC epoch seconds).
pub(crate) fn run(
    conn: &Connection,
    policy: RetentionPolicy,
    now: i64,
) -> Result<RetentionReport, LedgerError> {
    let day_start = local_day_start(now)?;
    let one_minute_cutoff = cutoff(now, policy.one_minute_days, day_start);
    let five_minute_cutoff = cutoff(now, policy.five_minute_days, day_start);
    let tx = conn.unchecked_transaction()?;
    let measurements_1m = tx.execute(
        "DELETE FROM measurements WHERE ts < ?1",
        params![one_minute_cutoff],
    )?;
    let measurements_5m = tx.execute(
        "DELETE FROM measurements_5m WHERE ts < ?1",
        params![five_minute_cutoff],
    )?;
    let (plans, plan_slots) = trim_plans(&tx, policy.plans_kept, day_start)?;
    let dispatch_log = tx.execute(
        "DELETE FROM dispatch_log
         WHERE ts < ?1 AND id NOT IN (
             SELECT id FROM dispatch_log ORDER BY ts DESC, id DESC LIMIT ?2
         )",
        params![day_start, policy.dispatch_kept.max(0)],
    )?;
    tx.commit()?;
    Ok(RetentionReport {
        measurements_1m,
        measurements_5m,
        plans,
        plan_slots,
        dispatch_log,
    })
}

/// Delete plans older than today beyond the newest `keep`, their slots first (explicit, so the
/// count is reportable even though the foreign key would cascade).
fn trim_plans(
    tx: &rusqlite::Transaction<'_>,
    keep: i64,
    day_start: i64,
) -> Result<(usize, usize), LedgerError> {
    let plan_ids = {
        let mut statement = tx.prepare(
            "SELECT id FROM plans
             WHERE created_at < ?1 AND id NOT IN (
                 SELECT id FROM plans ORDER BY created_at DESC, id DESC LIMIT ?2
             )",
        )?;
        let rows =
            statement.query_map(params![day_start, keep.max(0)], |row| row.get::<_, i64>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        ids
    };
    let mut plans = 0_usize;
    let mut plan_slots = 0_usize;
    {
        let mut delete_slots = tx.prepare("DELETE FROM plan_slots WHERE plan_id = ?1")?;
        let mut delete_plan = tx.prepare("DELETE FROM plans WHERE id = ?1")?;
        for id in plan_ids {
            plan_slots += delete_slots.execute(params![id])?;
            plans += delete_plan.execute(params![id])?;
        }
    }
    Ok((plans, plan_slots))
}

/// `now - days`, but never inside the current local day.
fn cutoff(now: i64, days: i64, day_start: i64) -> i64 {
    now.saturating_sub(days.max(0).saturating_mul(DAY_S))
        .min(day_start)
}

/// Start of the current local day in UTC epoch seconds.
fn local_day_start(now: i64) -> Result<i64, LedgerError> {
    let instant = Utc.timestamp_opt(now, 0).single().ok_or_else(|| {
        LedgerError::Invalid(format!("timestamp {now} is not representable as UTC"))
    })?;
    let local = instant.with_timezone(&Amsterdam);
    let midnight = local
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| LedgerError::Invalid("local midnight is not representable".to_owned()))?;
    let start = Amsterdam
        .from_local_datetime(&midnight)
        .earliest()
        .ok_or_else(|| LedgerError::Invalid("local midnight does not exist".to_owned()))?;
    Ok(start.timestamp())
}
