//! v4: the learned capacity/efficiency on the pack row, so the UI and the planner read one number.
//!
//! The learner's own profile (JSON, with provenance) stays in `settings`; these columns are the
//! three numbers the Batteries screen and the optimizer both need, and they are written whenever a
//! learning pass finishes. All nullable: an unlearned pack has no learned values.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 4,
    name: "pack-learning",
    sql: "
ALTER TABLE packs ADD COLUMN learned_capacity_kwh REAL;
ALTER TABLE packs ADD COLUMN learned_round_trip REAL;
ALTER TABLE packs ADD COLUMN learned_confidence REAL;
ALTER TABLE packs ADD COLUMN learned_at INTEGER;
",
};
