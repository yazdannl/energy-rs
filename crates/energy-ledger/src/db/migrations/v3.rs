//! v3: a human note on the dispatch log (the maintenance hold's balance-current explanation).

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 3,
    name: "dispatch-note",
    sql: "ALTER TABLE dispatch_log ADD COLUMN note TEXT;",
};
