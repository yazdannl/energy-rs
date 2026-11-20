//! v9: per-pack full-charge bookkeeping.
//!
//! A balancing charge is due seven days after the *previous dwell completed*, not at a wall-clock
//! time. Both instants live on the pack row: `last_full_charge_at` moves only after a successful
//! balancing dwell, and `dwell_started_at` tracks an in-progress one so a restart cannot restart
//! the clock and stretch a balance into an open-ended hold.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 9,
    name: "pack-full-charge-history",
    sql: "ALTER TABLE packs ADD COLUMN last_full_charge_at INTEGER;
          ALTER TABLE packs ADD COLUMN dwell_started_at INTEGER;",
};
