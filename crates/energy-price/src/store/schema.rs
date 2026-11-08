//! SQLite schema and in-code migrations.  Slot rows belong to one publication and are never
//! updated, so a past publication cannot be rewritten; only `fetched_at` moves, because it is the
//! last successful poll of that content, not part of it.
//!
//! `price_publications` holds the publication identity: `source` is the provider and resolution
//! (`zonneplan_quarter_hourly`, `ha_hourly`, ...), `content_hash` is the publication key (content
//! hash of the slot set), `last_t_end` is the coverage end used to detect a new publication, and
//! `fetched_at` is the last poll that returned this content (the staleness bound reads it).  `price_slots` keeps one row per slot per publication.

use rusqlite::Connection;

use crate::error::PriceError;

/// Current schema version (`PRAGMA user_version`).
pub(crate) const SCHEMA_VERSION: i64 = 2;

const MIGRATION_V1: &str = "
CREATE TABLE IF NOT EXISTS price_publications (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    source        TEXT    NOT NULL,
    content_hash  TEXT    NOT NULL UNIQUE,
    first_t_start INTEGER NOT NULL,
    last_t_end    INTEGER NOT NULL,
    slot_count    INTEGER NOT NULL,
    fetched_at    INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS price_slots (
    publication_id INTEGER NOT NULL REFERENCES price_publications(id) ON DELETE CASCADE,
    t_start        INTEGER NOT NULL,
    buy_all_in     REAL    NOT NULL,
    market         REAL    NOT NULL,
    source         TEXT    NOT NULL,
    PRIMARY KEY (publication_id, t_start)
);
CREATE INDEX IF NOT EXISTS price_slots_by_start ON price_slots(t_start);
";

/// v2: the published price without the energy tax, the feed-in price once net metering ends.
/// Nullable, so a v1 row keeps the fallback (`market + fee`) instead of inventing a number.
const MIGRATION_V2: &str = "ALTER TABLE price_slots ADD COLUMN buy_ex_tax REAL;";

/// Apply pending migrations in code.
pub(crate) fn migrate(conn: &Connection) -> Result<(), PriceError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 1 {
        conn.execute_batch(MIGRATION_V1)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    if version < 2 {
        conn.execute_batch(MIGRATION_V2)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}
