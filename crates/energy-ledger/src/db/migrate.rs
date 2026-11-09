//! Forward-only migration runner. The applied version lives in `meta.schema_version`.
//!
//! The `meta` table itself is created before the numbered migrations, so an empty file and a file
//! written by any prior version follow the same code path. Each migration runs in its own
//! transaction and records its version in that transaction: a crash leaves either the old schema
//! plus its version or the new schema plus its version, never a half-applied step.
//!
//! A database from the future (version > this binary's) is refused instead of opened.

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::migration::Migration;
use crate::db::migrations::MIGRATIONS;
use crate::error::LedgerError;

/// Current schema version; adding a migration bumps this and appends to the list.
pub(crate) const SCHEMA_VERSION: i64 = 6;

const VERSION_KEY: &str = "schema_version";

const META_TABLE: &str = "
CREATE TABLE IF NOT EXISTS meta (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) WITHOUT ROWID;
";

/// Migrate to the version this binary supports.
pub(crate) fn run(conn: &mut Connection) -> Result<(), LedgerError> {
    run_to(conn, SCHEMA_VERSION)
}

/// Migrate up to `target` (inclusive), used by the upgrade tests and by nothing else.
pub(crate) fn run_to(conn: &mut Connection, target: i64) -> Result<(), LedgerError> {
    if target > SCHEMA_VERSION {
        return Err(LedgerError::SchemaTooNew {
            found: target,
            supported: SCHEMA_VERSION,
        });
    }
    conn.execute_batch(META_TABLE)?;
    let current = current_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(LedgerError::SchemaTooNew {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    let now = Utc::now().timestamp();
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > current && migration.version <= target)
    {
        apply(conn, migration, now)?;
    }
    Ok(())
}

/// The stored schema version (`0` when the file has never been migrated).
pub(crate) fn current_version(conn: &Connection) -> Result<i64, LedgerError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![VERSION_KEY],
            |row| row.get(0),
        )
        .optional()?;
    let Some(text) = raw else {
        return Ok(0);
    };
    text.parse::<i64>()
        .map_err(|_| LedgerError::Invalid(format!("meta.schema_version is {text:?}, not a number")))
}

/// Refuse a file this binary cannot read, without migrating it.
pub(crate) fn verify(conn: &Connection) -> Result<i64, LedgerError> {
    let current = current_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(LedgerError::SchemaTooNew {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(current)
}

fn apply(conn: &mut Connection, migration: &Migration, now: i64) -> Result<(), LedgerError> {
    let tx = conn.transaction()?;
    tx.execute_batch(migration.sql)
        .map_err(|err| LedgerError::Migrate {
            version: migration.version,
            reason: err.to_string(),
        })?;
    tx.execute(
        "INSERT INTO meta (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![VERSION_KEY, migration.version.to_string(), now],
    )?;
    tx.commit()?;
    tracing::debug!(
        version = migration.version,
        name = migration.name,
        "ledger migration applied"
    );
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn migrated_to(target: i64) -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        run_to(&mut conn, target).unwrap();
        conn
    }

    #[test]
    fn migrates_from_empty_to_current() {
        let conn = migrated_to(SCHEMA_VERSION);
        assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
        let stored: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, SCHEMA_VERSION.to_string());
    }

    #[test]
    fn migrates_from_every_prior_version_without_losing_rows() {
        for target in 0..SCHEMA_VERSION {
            let mut conn = migrated_to(target);
            if target >= 1 {
                conn.execute(
                    "INSERT INTO packs (sn, label_capacity_kwh, first_seen, last_seen)
                     VALUES ('ABC123', 1.92, 100, 200)",
                    [],
                )
                .unwrap();
            }
            run(&mut conn).unwrap();
            assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
            if target >= 1 {
                let packs: i64 = conn
                    .query_row("SELECT COUNT(*) FROM packs", [], |row| row.get(0))
                    .unwrap();
                assert_eq!(packs, 1);
            }
            let tables: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'measurements_5m'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(tables, 1);
        }
    }

    #[test]
    fn v5_keeps_old_savings_money_and_leaves_the_split_null() {
        let mut conn = migrated_to(4);
        conn.execute(
            "INSERT INTO savings_daily
                (day_local, actual_eur, baseline_eur, saving_eur, import_kwh, export_kwh, pv_kwh,
                 battery_charge_kwh, battery_discharge_kwh, coverage_hours, complete)
             VALUES ('2026-06-15', 0.5, 1.0, 0.5, 2.0, 1.0, 3.0, 1.0, 0.9, 24.0, 1)",
            [],
        )
        .unwrap();
        run(&mut conn).unwrap();
        let (actual, saving, split, complete): (f64, f64, Option<f64>, i64) = conn
            .query_row(
                "SELECT actual_eur, saving_eur, arbitrage_eur, complete FROM savings_daily",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert!((actual - 0.5).abs() < 1e-12);
        assert!((saving - 0.5).abs() < 1e-12);
        assert_eq!(split, None);
        assert_eq!(complete, 1);
    }

    #[test]
    fn refuses_a_database_from_the_future() {
        let mut conn = migrated_to(SCHEMA_VERSION);
        conn.execute(
            "UPDATE meta SET value = '99' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
        assert!(matches!(
            run(&mut conn),
            Err(LedgerError::SchemaTooNew {
                found: 99,
                supported: SCHEMA_VERSION
            })
        ));
    }

    #[test]
    fn run_to_does_not_skip_versions() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_to(&mut conn, 1).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 1);
        run_to(&mut conn, 1).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 1);
    }
}
