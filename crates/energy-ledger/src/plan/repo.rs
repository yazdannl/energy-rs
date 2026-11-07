//! Plan persistence: one transaction per plan, newest-plan reads and range reads.

use std::sync::MutexGuard;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::LedgerError;
use crate::plan::{Plan, PlanSlot};

const PLAN_COLUMNS: &str = "id, created_at, mode, horizon_start, horizon_end, summary_json";

const SLOT_COLUMNS: &str = "plan_id, idx, t_start, buy_all_in, feed_in, pv_kw, load_kw, \
                            battery_kw, soc_kwh, target_soc_kwh, reason";

/// Repository for the `plans` and `plan_slots` tables.
pub struct Plans<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> Plans<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// Store a plan header and its slots in one transaction, returning the stored header.
    pub fn insert(&self, plan: &Plan, slots: &[PlanSlot]) -> Result<Plan, LedgerError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO plans (created_at, mode, horizon_start, horizon_end, summary_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                plan.created_at,
                plan.mode,
                plan.horizon_start,
                plan.horizon_end,
                plan.summary_json,
            ],
        )?;
        let id = tx.last_insert_rowid();
        {
            let mut insert = tx.prepare(&format!(
                "INSERT INTO plan_slots ({SLOT_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
            ))?;
            for slot in slots {
                insert.execute(params![
                    id,
                    slot.idx,
                    slot.t_start,
                    slot.buy_all_in,
                    slot.feed_in,
                    slot.pv_kw,
                    slot.load_kw,
                    slot.battery_kw,
                    slot.soc_kwh,
                    slot.target_soc_kwh,
                    slot.reason,
                ])?;
            }
        }
        tx.commit()?;
        let mut stored = plan.clone();
        stored.id = id;
        Ok(stored)
    }

    /// The newest stored plan (the one the UI and the engine display).
    pub fn current(&self) -> Result<Option<Plan>, LedgerError> {
        let sql =
            format!("SELECT {PLAN_COLUMNS} FROM plans ORDER BY created_at DESC, id DESC LIMIT 1");
        let plan = self.conn.query_row(&sql, [], Plan::from_row).optional()?;
        Ok(plan)
    }

    /// Plans created in `[from, to)`, ascending.
    pub fn range(&self, from: i64, to: i64) -> Result<Vec<Plan>, LedgerError> {
        let sql = format!(
            "SELECT {PLAN_COLUMNS} FROM plans
             WHERE created_at >= ?1 AND created_at < ?2 ORDER BY created_at, id"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![from, to], Plan::from_row)?;
        let mut plans = Vec::new();
        for row in rows {
            plans.push(row?);
        }
        Ok(plans)
    }

    /// The slots of one plan, ordered by their index.
    pub fn slots(&self, plan_id: i64) -> Result<Vec<PlanSlot>, LedgerError> {
        let sql = format!("SELECT {SLOT_COLUMNS} FROM plan_slots WHERE plan_id = ?1 ORDER BY idx");
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![plan_id], PlanSlot::from_row)?;
        let mut slots = Vec::new();
        for row in rows {
            slots.push(row?);
        }
        Ok(slots)
    }
}
