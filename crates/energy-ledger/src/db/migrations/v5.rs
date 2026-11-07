//! v5: savings money may be unknown, and the day carries its four-way split.
//!
//! A closed day with a measurement gap stores `NULL` money (never a made-up number) while keeping
//! its measured energy and `coverage_hours`. The split columns (`arbitrage`, self-consumption,
//! feed-in, battery balance) are written for every day that has money and stay `NULL` for a day that
//! does not; rows written before this migration keep their money and get `NULL` splits, because the
//! per-slot detail needed to split them was never stored.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 5,
    name: "savings-split",
    sql: "
CREATE TABLE savings_daily_v5 (
    day_local             TEXT PRIMARY KEY,
    actual_eur            REAL,
    baseline_eur          REAL,
    saving_eur            REAL,
    arbitrage_eur         REAL,
    self_consumption_eur  REAL,
    feed_in_eur           REAL,
    battery_balance_eur   REAL,
    import_kwh            REAL NOT NULL,
    export_kwh            REAL NOT NULL,
    pv_kwh                REAL NOT NULL,
    battery_charge_kwh    REAL NOT NULL,
    battery_discharge_kwh REAL NOT NULL,
    coverage_hours        REAL NOT NULL,
    complete              INTEGER NOT NULL CHECK (complete IN (0, 1))
) WITHOUT ROWID;

INSERT INTO savings_daily_v5
    (day_local, actual_eur, baseline_eur, saving_eur, arbitrage_eur, self_consumption_eur,
     feed_in_eur, battery_balance_eur, import_kwh, export_kwh, pv_kwh, battery_charge_kwh,
     battery_discharge_kwh, coverage_hours, complete)
SELECT day_local, actual_eur, baseline_eur, saving_eur, NULL, NULL, NULL, NULL,
       import_kwh, export_kwh, pv_kwh, battery_charge_kwh, battery_discharge_kwh,
       coverage_hours, complete
FROM savings_daily;

DROP TABLE savings_daily;
ALTER TABLE savings_daily_v5 RENAME TO savings_daily;
",
};
