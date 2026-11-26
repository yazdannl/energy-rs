//! v14: the clocks a no-flow verdict judged, so a false hold is diagnosable.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 14,
    name: "dispatch-flow-verdict-clocks",
    sql: "ALTER TABLE dispatch_log ADD COLUMN judged_sample_at INTEGER;
          ALTER TABLE dispatch_log ADD COLUMN judged_write_at INTEGER;",
};
