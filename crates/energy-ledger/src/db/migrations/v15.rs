//! v15: the operator's per-pack AC round trip, entered before learning has a confident value.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 15,
    name: "pack-manual-round-trip",
    sql: "ALTER TABLE packs ADD COLUMN manual_round_trip REAL;",
};
