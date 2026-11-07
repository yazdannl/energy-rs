//! v1: packs, the 1-minute measurement tier and the extracted sessions.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 1,
    name: "packs-measurements-sessions",
    sql: "
CREATE TABLE packs (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    sn                    TEXT    NOT NULL UNIQUE,
    name                  TEXT,
    label_capacity_kwh    REAL    NOT NULL,
    reported_capacity_kwh REAL,
    max_charge_w          REAL,
    max_discharge_w       REAL,
    first_seen            INTEGER NOT NULL,
    last_seen             INTEGER NOT NULL
);

CREATE TABLE measurements (
    pack_id INTEGER NOT NULL REFERENCES packs(id) ON DELETE CASCADE,
    ts      INTEGER NOT NULL,
    power_w REAL    NOT NULL,
    soc_pct REAL,
    kind    TEXT    NOT NULL DEFAULT 'report',
    PRIMARY KEY (pack_id, ts)
) WITHOUT ROWID;

CREATE TABLE sessions (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    pack_id            INTEGER NOT NULL REFERENCES packs(id) ON DELETE CASCADE,
    direction          TEXT    NOT NULL CHECK (direction IN ('charge', 'discharge')),
    started_at         INTEGER NOT NULL,
    ended_at           INTEGER NOT NULL,
    soc_start          REAL,
    soc_end            REAL,
    energy_ac_kwh      REAL    NOT NULL,
    duration_s         INTEGER NOT NULL,
    sample_count       INTEGER NOT NULL,
    complete           INTEGER NOT NULL CHECK (complete IN (0, 1)),
    invalidated_reason TEXT,
    UNIQUE (pack_id, started_at)
);

CREATE INDEX sessions_by_pack_start ON sessions(pack_id, started_at);
",
};
