//! The v15 provenance backfill: a current stamp is not proof of a measured round trip.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::{Connection, params};

use super::migrate::run;

fn migrated_to(target: i64) -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    super::migrate::run_to(&mut conn, target).unwrap();
    conn
}

#[test]
fn v15_backfills_unmeasured_packs_from_the_durable_profile() {
    let mut conn = migrated_to(14);
    let now = 1_700_000_000;
    for sn in ["SN-SEED", "SN-MEASURED", "SN-MISSING", "SN-CORRUPT"] {
        conn.execute(
            "INSERT INTO packs
                (sn, label_capacity_kwh, learned_round_trip, learned_at, first_seen, last_seen)
             VALUES (?1, 1.92, 0.81, ?2, 100, 200)",
            params![sn, now],
        )
        .unwrap();
    }
    // The durable profiles: a typed seed, a real measurement, and one unreadable document.
    for (sn, value) in [
        ("SN-SEED", r#"{"efficiency_applied":false}"#),
        ("SN-MEASURED", r#"{"efficiency_applied":true}"#),
        ("SN-CORRUPT", "{not json"),
    ] {
        conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
            params![format!("learning.profile.{sn}"), value, now],
        )
        .unwrap();
    }

    run(&mut conn).unwrap();
    let corrupt_valid: i64 = conn
        .query_row(
            "SELECT json_valid(value) FROM settings
             WHERE key = 'learning.profile.SN-CORRUPT'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        corrupt_valid, 0,
        "the fixture must be genuinely malformed JSON"
    );
    for sn in ["SN-SEED", "SN-MISSING", "SN-CORRUPT"] {
        let (stamp, manual): (Option<i64>, Option<f64>) = conn
            .query_row(
                "SELECT learned_at, manual_round_trip FROM packs WHERE sn = ?1",
                params![sn],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stamp, Some(0), "{sn} has no proof of a measured round trip");
        assert_eq!(manual, None, "migration must not invent an operator entry");
    }
    let measured: i64 = conn
        .query_row(
            "SELECT learned_at FROM packs WHERE sn = 'SN-MEASURED'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(measured, now, "a published measurement keeps its stamp");
}
