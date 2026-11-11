//! v8: the operator's alias for a pack.
//!
//! Discovery owns `name` (what Home Assistant calls the device) and re-registers it on every poll.
//! The alias is the operator's own label, written only through `PATCH /api/v1/packs/{sn}`, so it
//! lives in its own column: an upsert never touches it and a renamed pack keeps its alias.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 8,
    name: "pack-alias",
    sql: "ALTER TABLE packs ADD COLUMN alias TEXT;",
};
