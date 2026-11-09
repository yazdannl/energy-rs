//! The ledger: everything the system remembers.
//!
//! Responsibility (PLAN.md §2, §8): own SQLite at `/data/energy_rs.db` (WAL) and be the only
//! writer of the operational tables. Time is UTC epoch seconds; energy is kWh (f64); money is EUR
//! (f64, rounded to 1e-6 on write); local days are `Europe/Amsterdam` dates (`YYYY-MM-DD`).
//!
//! # Table ownership
//!
//! The ledger owns and migrates **only** these tables:
//!
//! | table | content |
//! |---|---|
//! | `packs` | one row per battery pack: serial, label/reported capacity, power maxima, first/last seen |
//! | `measurements` | 1-minute pack samples `(pack_id, ts)` upsert, `power_w`, `soc_pct`, `kind` |
//! | `measurements_5m` | 5-minute rollup of the 1-minute tier (means, `NULL` when a bucket has no samples) |
//! | `sessions` | charge/discharge runs extracted from a measurement tier, with flags and completeness |
//! | `plans` / `plan_slots` | every generated plan and its 15-minute timeline |
//! | `dispatch_log` | every requested/applied write, including dry runs and rejections |
//! | `events` | the journal: level, kind, message and an opaque JSON payload |
//! | `savings_daily` | the per-local-day accounting row users see |
//! | `meta` | schema version and run metadata |
//! | `settings` / `settings_history` | current settings plus the as-of history |
//!
//! Price tables (`price_publications`, `price_slots`) belong to `energy-price`; forecast caches
//! belong to `energy-forecast`. The ledger never creates, migrates or writes them, so the two
//! stores can share one file without owning each other's schema.
//!
//! # Retention
//!
//! 1-minute rows for 30 days, 5-minute rows for 1 year, savings daily rows forever, plans trimmed
//! to the newest 200, dispatch rows to the newest 20 000 and journal events to the newest 5 000.
//! Sessions are kept: capacity learning
//! weights the last sessions most but needs a long tail to stay honest. A trim never deletes a row
//! whose timestamp falls inside the current local day, and the 5-minute rollup never replaces a
//! populated bucket with an empty one recomputed after 1-minute retention.
//!
//! # Sessions
//!
//! A session is a maximal same-direction run of at least [`SessionRules::min_samples`] samples with
//! `|power| >= deadband`, and energy is the mean AC power times the interval the samples actually
//! cover. A run is `complete` only when a hold or opposite sample bounds it on both sides; SoC gaps
//! and one-sample direction reversals are stored flagged (`invalidated_reason`) so they can be
//! audited but never learned from. Extraction deletes and rewrites the sessions whose start falls
//! in the requested window, which makes it deterministic and re-runnable.
//!
//! # Modules
//!
//! * [`db`] — connection handle, pragmas, forward-only migrations.
//! * [`pack`] — packs and latest pack state.
//! * [`measurement`] — 1-minute samples, 5-minute rollup, range reads.
//! * [`session`] — session extraction (charge/discharge runs).
//! * [`plan`] / [`dispatch`] — plan + slot and dispatch-log persistence.
//! * [`event`] / [`savings`] / [`settings`] — journal, daily accounting, as-of settings.
//! * [`retention`] — the trim job.
//! * [`stats`] — read-only row counts and freshness for the doctor.

pub mod db;
pub mod dispatch;
pub mod error;
pub mod event;
pub mod measurement;
pub mod pack;
pub mod plan;
pub mod pool;
pub mod retention;
pub mod savings;
pub mod selection;
pub mod session;
pub mod settings;
pub mod stats;

pub use db::{Ledger, LedgerConfig};
pub use dispatch::DispatchEntry;
pub use error::LedgerError;
pub use event::{Event, EventLevel};
pub use measurement::{Measurement, RollupReport, Sample, SampleKind, Tier};
pub use pack::{Pack, PackState};
pub use plan::{Plan, PlanSlot};
pub use retention::{RetentionPolicy, RetentionReport};
pub use savings::SavingsDaily;
pub use session::{Direction, ExtractReport, Session, SessionRules};
pub use stats::LedgerStats;
