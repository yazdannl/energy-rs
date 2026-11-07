//! Savings persistence.

use std::sync::MutexGuard;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::LedgerError;
use crate::savings::daily::{SavingsDaily, round_opt};

const COLUMNS: &str = "day_local, actual_eur, baseline_eur, saving_eur, arbitrage_eur, \
                       self_consumption_eur, feed_in_eur, battery_balance_eur, import_kwh, \
                       export_kwh, pv_kwh, battery_charge_kwh, battery_discharge_kwh, \
                       coverage_hours, complete";

/// Repository for the `savings_daily` table.
pub struct Savings<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> Savings<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// Insert or replace one local day, rounding its money columns to 1e-6.
    ///
    /// The upsert makes a recomputation idempotent: the same measurements produce the same row, and
    /// a `NULL` money column replaces a previously stored number for a day that lost coverage.
    pub fn upsert(&self, day: &SavingsDaily) -> Result<(), LedgerError> {
        self.conn.execute(
            "INSERT INTO savings_daily
                (day_local, actual_eur, baseline_eur, saving_eur, arbitrage_eur,
                 self_consumption_eur, feed_in_eur, battery_balance_eur, import_kwh, export_kwh,
                 pv_kwh, battery_charge_kwh, battery_discharge_kwh, coverage_hours, complete)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(day_local) DO UPDATE SET
                actual_eur = excluded.actual_eur,
                baseline_eur = excluded.baseline_eur,
                saving_eur = excluded.saving_eur,
                arbitrage_eur = excluded.arbitrage_eur,
                self_consumption_eur = excluded.self_consumption_eur,
                feed_in_eur = excluded.feed_in_eur,
                battery_balance_eur = excluded.battery_balance_eur,
                import_kwh = excluded.import_kwh,
                export_kwh = excluded.export_kwh,
                pv_kwh = excluded.pv_kwh,
                battery_charge_kwh = excluded.battery_charge_kwh,
                battery_discharge_kwh = excluded.battery_discharge_kwh,
                coverage_hours = excluded.coverage_hours,
                complete = excluded.complete",
            params![
                day.day_local,
                round_opt(day.actual_eur),
                round_opt(day.baseline_eur),
                round_opt(day.saving_eur),
                round_opt(day.arbitrage_eur),
                round_opt(day.self_consumption_eur),
                round_opt(day.feed_in_eur),
                round_opt(day.battery_balance_eur),
                day.import_kwh,
                day.export_kwh,
                day.pv_kwh,
                day.battery_charge_kwh,
                day.battery_discharge_kwh,
                day.coverage_hours,
                day.complete,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, day_local: &str) -> Result<Option<SavingsDaily>, LedgerError> {
        let sql = format!("SELECT {COLUMNS} FROM savings_daily WHERE day_local = ?1");
        let day = self
            .conn
            .query_row(&sql, params![day_local], SavingsDaily::from_row)
            .optional()?;
        Ok(day)
    }

    /// Days in `[from_day, to_day]` (inclusive ISO dates, lexically ordered), ascending.
    pub fn range(&self, from_day: &str, to_day: &str) -> Result<Vec<SavingsDaily>, LedgerError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM savings_daily
             WHERE day_local >= ?1 AND day_local <= ?2 ORDER BY day_local"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![from_day, to_day], SavingsDaily::from_row)?;
        let mut days = Vec::new();
        for row in rows {
            days.push(row?);
        }
        Ok(days)
    }

    /// The newest stored day strictly before `today`; the rollup's resume point.
    pub fn newest_closed(&self, today: &str) -> Result<Option<SavingsDaily>, LedgerError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM savings_daily WHERE day_local < ?1
             ORDER BY day_local DESC LIMIT 1"
        );
        let day = self
            .conn
            .query_row(&sql, params![today], SavingsDaily::from_row)
            .optional()?;
        Ok(day)
    }

    /// Every stored day in one calendar year, ascending (the lifetime/year read).
    pub fn year(&self, year: i32) -> Result<Vec<SavingsDaily>, LedgerError> {
        let from = format!("{year:04}-01-01");
        let to = format!("{year:04}-12-31");
        self.range(&from, &to)
    }
}
