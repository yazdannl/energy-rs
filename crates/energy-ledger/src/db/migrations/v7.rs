//! v7: the latched pack selection.
//!
//! One row per fleet: which pack was picked for the current charge or discharge cycle and since
//! when. A restart in the middle of a cycle must not rotate the choice, and the hold has to expire
//! on its own, so the row carries its own timestamp (`docs/PACK_SELECTION.md` §5).

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 7,
    name: "pack-selection",
    sql: "
CREATE TABLE pack_selection (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    direction   TEXT    NOT NULL CHECK (direction IN ('charge', 'discharge')),
    holder      TEXT    NOT NULL,
    selected_at INTEGER NOT NULL,
    target_w    REAL    NOT NULL
);
",
};
