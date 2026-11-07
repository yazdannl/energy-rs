//! The ledger: everything the system remembers.
//!
//! Responsibility (PLAN.md §2, §8): own SQLite at `/data/energy_rs.db` (WAL) through a single
//! `tokio-rusqlite` connection actor, and be the only writer.
//!
//! * [`db`] — connection actor, pragmas, transaction helpers and shutdown checkpoint.
//! * [`migrations`] — forward-only schema migrations with a version table.
//! * [`savings`] — per-slot `actual_cost` vs `baseline_cost` (no-battery counterfactual) and the
//!   daily/lifetime rollups, always derived from ledger rows and never from planned values.
//!
//! Retention: 30 days of 1-minute rows, 1 year of 5-minute rows, daily totals forever.
//! Time is UTC epoch seconds; energy Wh (f64); money EUR (f64, rounded to 1e-6 on write).

pub mod db;
pub mod migrations;
pub mod savings;
