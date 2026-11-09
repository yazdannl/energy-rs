//! v6: the stored-energy cost pool.
//!
//! One row: the average cost of the energy in the pack, advanced from measured charge/discharge
//! flows (`docs/POLICY.md` §5). A missing row means "no basis yet", which the planner reports as a
//! degraded plan rather than guessing a number.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 6,
    name: "energy-pool",
    sql: "
CREATE TABLE energy_pool (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    kwh        REAL    NOT NULL,
    cost_eur   REAL    NOT NULL,
    updated_at INTEGER NOT NULL
);
",
};
