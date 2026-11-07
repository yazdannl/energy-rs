//! v2: the 5-minute tier, plans, dispatch log, events, savings and settings.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 2,
    name: "plans-dispatch-savings-settings",
    sql: "
CREATE TABLE measurements_5m (
    pack_id INTEGER NOT NULL REFERENCES packs(id) ON DELETE CASCADE,
    ts      INTEGER NOT NULL,
    power_w REAL,
    soc_pct REAL,
    samples INTEGER NOT NULL,
    PRIMARY KEY (pack_id, ts)
) WITHOUT ROWID;

CREATE TABLE plans (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at    INTEGER NOT NULL,
    mode          TEXT    NOT NULL,
    horizon_start INTEGER NOT NULL,
    horizon_end   INTEGER NOT NULL,
    summary_json  TEXT    NOT NULL DEFAULT '{}'
);

CREATE INDEX plans_by_created ON plans(created_at);

CREATE TABLE plan_slots (
    plan_id        INTEGER NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    idx            INTEGER NOT NULL,
    t_start        INTEGER NOT NULL,
    buy_all_in     REAL    NOT NULL,
    feed_in        REAL    NOT NULL,
    pv_kw          REAL    NOT NULL,
    load_kw        REAL    NOT NULL,
    battery_kw     REAL    NOT NULL,
    soc_kwh        REAL    NOT NULL,
    target_soc_kwh REAL    NOT NULL,
    reason         TEXT    NOT NULL DEFAULT '',
    PRIMARY KEY (plan_id, idx)
) WITHOUT ROWID;

CREATE TABLE dispatch_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    ts          INTEGER NOT NULL,
    pack_id     INTEGER REFERENCES packs(id) ON DELETE SET NULL,
    requested_w REAL    NOT NULL,
    applied_w   REAL    NOT NULL,
    reason      TEXT    NOT NULL,
    mode        TEXT    NOT NULL,
    verified    INTEGER NOT NULL CHECK (verified IN (0, 1)),
    error       TEXT
);

CREATE INDEX dispatch_by_ts ON dispatch_log(ts);

CREATE TABLE events (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    ts        INTEGER NOT NULL,
    level     TEXT    NOT NULL,
    kind      TEXT    NOT NULL,
    message   TEXT    NOT NULL,
    data_json TEXT
);

CREATE INDEX events_by_ts ON events(ts);

CREATE TABLE savings_daily (
    day_local             TEXT PRIMARY KEY,
    actual_eur            REAL NOT NULL,
    baseline_eur          REAL NOT NULL,
    saving_eur            REAL NOT NULL,
    import_kwh            REAL NOT NULL,
    export_kwh            REAL NOT NULL,
    pv_kwh                REAL NOT NULL,
    battery_charge_kwh    REAL NOT NULL,
    battery_discharge_kwh REAL NOT NULL,
    coverage_hours        REAL NOT NULL,
    complete              INTEGER NOT NULL CHECK (complete IN (0, 1))
) WITHOUT ROWID;

CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE settings_history (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    valid_from INTEGER NOT NULL
);

CREATE INDEX settings_history_asof ON settings_history(key, valid_from);
",
};
