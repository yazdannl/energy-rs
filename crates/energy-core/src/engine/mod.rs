//! The engine: one task that owns every control decision.
//!
//! * [`config`](crate::engine::EngineConfig) — tick rate, mode, fault policy, grid caps and the
//!   protection settings.
//! * [`engine`](crate::engine::Engine) — the loop's public surface: `boot`, `tick_once`,
//!   `supervise`, `shutdown`, mode and override changes.
//! * [`watchdog`](crate::engine::Watchdog) — progress marks, not tick boundaries: a tick that keeps
//!   reading and writing is alive however long four sequential read-backs take (survey §7 trap 10).
//! * [`live_latch`](crate::engine::LiveLatch) — the persisted record of what we left running; only a
//!   *verified* hold clears it, and boot only ever neutralises locks that are in it.
//! * [`plan_source`](crate::engine::PlanSource) — where a new 120-slot plan comes from.
//!
//! The engine never blocks a tick on its own lock: I/O happens before and after, and the state
//! mutex is only taken for the synchronous parts.

mod config;
mod control;
mod error;
mod latched_write;
mod live_latch;
mod persist;
mod plan_source;
mod run;
mod state;
mod supervise;
mod tick;
mod watchdog;

pub use config::EngineConfig;
pub use control::Engine;
pub use error::EngineError;
pub use latched_write::LatchedWrite;
pub use live_latch::LiveLatch;
pub use plan_source::PlanSource;
pub use watchdog::Watchdog;

use chrono::{DateTime, Utc};

/// A setpoint at or below this many watts counts as neutral for the live-write latch.
pub const LATCH_TOLERANCE_W: f64 = 30.0;
/// Ledger settings key holding the live-write latch.
pub const LIVE_LATCH_KEY: &str = "control.live_writes";

/// UTC instant for an epoch-seconds value; an impossible value becomes the Unix epoch.
pub(crate) fn instant(now: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(now, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}
