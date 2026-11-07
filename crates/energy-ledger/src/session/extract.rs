//! Loading a window, scanning it and replacing the sessions it owns.

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::LedgerError;
use crate::measurement::Tier;
use crate::session::report::ExtractReport;
use crate::session::rules::SessionRules;
use crate::session::scan::{self, Scan, ScanSample};

/// Extract and store one pack's sessions for `[from, to)`.
pub(crate) fn run(
    conn: &Connection,
    pack_id: i64,
    tier: Tier,
    from: i64,
    to: i64,
    rules: &SessionRules,
) -> Result<ExtractReport, LedgerError> {
    if from >= to {
        return Ok(ExtractReport::default());
    }
    let before = context(conn, pack_id, tier, from, true)?;
    let after = context(conn, pack_id, tier, to, false)?;
    let samples = load(conn, pack_id, tier, from, to)?;
    let scan = scan::scan(
        &samples,
        before.as_ref(),
        after.as_ref(),
        rules,
        tier.interval_s(),
    );
    store(conn, pack_id, from, to, &scan)
}

fn context(
    conn: &Connection,
    pack_id: i64,
    tier: Tier,
    boundary: i64,
    before: bool,
) -> Result<Option<ScanSample>, LedgerError> {
    let table = tier.table();
    let (comparison, order) = if before { ("<", "DESC") } else { (">=", "ASC") };
    let sql = format!(
        "SELECT ts, power_w, soc_pct FROM {table}
         WHERE pack_id = ?1 AND ts {comparison} ?2 ORDER BY ts {order} LIMIT 1"
    );
    let sample = conn
        .query_row(&sql, params![pack_id, boundary], |row| {
            Ok(ScanSample {
                ts: row.get(0)?,
                power_w: row.get(1)?,
                soc_pct: row.get(2)?,
            })
        })
        .optional()?;
    Ok(sample)
}

fn load(
    conn: &Connection,
    pack_id: i64,
    tier: Tier,
    from: i64,
    to: i64,
) -> Result<Vec<ScanSample>, LedgerError> {
    let table = tier.table();
    let sql = format!(
        "SELECT ts, power_w, soc_pct FROM {table}
         WHERE pack_id = ?1 AND ts >= ?2 AND ts < ?3 AND power_w IS NOT NULL ORDER BY ts"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params![pack_id, from, to], |row| {
        Ok(ScanSample {
            ts: row.get(0)?,
            power_w: row.get(1)?,
            soc_pct: row.get(2)?,
        })
    })?;
    let mut samples = Vec::new();
    for row in rows {
        samples.push(row?);
    }
    Ok(samples)
}

fn store(
    conn: &Connection,
    pack_id: i64,
    from: i64,
    to: i64,
    scan: &Scan,
) -> Result<ExtractReport, LedgerError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM sessions WHERE pack_id = ?1 AND started_at >= ?2 AND started_at < ?3",
        params![pack_id, from, to],
    )?;
    let mut report = ExtractReport {
        skipped: scan.skipped,
        ..ExtractReport::default()
    };
    {
        let mut insert = tx.prepare(
            "INSERT INTO sessions
                (pack_id, direction, started_at, ended_at, soc_start, soc_end, energy_ac_kwh,
                 duration_s, sample_count, complete, invalidated_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        for run in &scan.runs {
            let sample_count = i64::try_from(run.sample_count).map_err(|_| {
                LedgerError::Invalid("session sample count is too large".to_owned())
            })?;
            insert.execute(params![
                pack_id,
                run.direction.as_str(),
                run.start_ts,
                run.end_ts,
                run.soc_start,
                run.soc_end,
                run.energy_ac_kwh,
                run.end_ts - run.start_ts,
                sample_count,
                run.complete,
                run.invalidated_reason,
            ])?;
            report.written += 1;
            if run.invalidated_reason.is_some() {
                report.flagged += 1;
            }
        }
    }
    tx.commit()?;
    Ok(report)
}
