//! Session rules: bounded runs, partial edges, reversals, SoC gaps, deadband and re-runs.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::manual_midpoint
)]

mod support;

use energy_ledger::{Direction, Ledger, Session, SessionRules, Tier};
use support::{TempDatabase, register_pack, sample, write};

const CHARGE_W: f64 = 1_000.0;
/// 1000 W for 300 s: 0.0833 kWh.
const CHARGE_300_S_KWH: f64 = CHARGE_W * 300.0 / 3_600_000.0;

fn extract(ledger: &Ledger, pack: i64, from: i64, to: i64) -> Vec<Session> {
    ledger
        .sessions()
        .unwrap()
        .extract(pack, Tier::OneMinute, from, to, &SessionRules::default())
        .unwrap();
    ledger.sessions().unwrap().for_pack(pack, from, to).unwrap()
}

#[test]
fn a_bounded_run_is_complete_and_books_its_measured_energy() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, 0, 0.0, Some(49.0)),
            sample(pack, 60, CHARGE_W, Some(50.0)),
            sample(pack, 120, CHARGE_W, Some(51.0)),
            sample(pack, 180, CHARGE_W, Some(52.0)),
            sample(pack, 240, CHARGE_W, Some(53.0)),
            sample(pack, 300, CHARGE_W, Some(54.0)),
            sample(pack, 360, 0.0, Some(54.5)),
        ],
    );
    let sessions = extract(&ledger, pack, 0, 420);
    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.direction, Direction::Charge);
    assert_eq!(session.started_at, 60);
    assert_eq!(session.ended_at, 360);
    assert_eq!(session.duration_s, 300);
    assert_eq!(session.sample_count, 5);
    assert_eq!(session.soc_start, Some(50.0));
    assert_eq!(session.soc_end, Some(54.0));
    assert!(session.complete);
    assert_eq!(session.invalidated_reason, None);
    assert!((session.energy_ac_kwh - CHARGE_300_S_KWH).abs() < 1e-12);
}

#[test]
fn a_window_edge_is_incomplete_and_stops_at_the_next_sample() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, 60, CHARGE_W, Some(50.0)),
            sample(pack, 120, CHARGE_W, Some(51.0)),
            sample(pack, 180, CHARGE_W, Some(52.0)),
            sample(pack, 240, CHARGE_W, Some(53.0)),
            sample(pack, 300, CHARGE_W, Some(54.0)),
            sample(pack, 360, 0.0, Some(54.5)),
        ],
    );
    let sessions = extract(&ledger, pack, 60, 420);
    assert_eq!(sessions.len(), 1);
    assert!(!sessions[0].complete);
    assert_eq!(sessions[0].started_at, 60);
    assert_eq!(sessions[0].ended_at, 360);
    assert!((sessions[0].energy_ac_kwh - CHARGE_300_S_KWH).abs() < 1e-12);
}

#[test]
fn a_discharge_run_is_negative_power_and_positive_energy() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, 0, 0.0, Some(80.0)),
            sample(pack, 60, -500.0, Some(79.0)),
            sample(pack, 120, -500.0, Some(78.0)),
            sample(pack, 180, 0.0, Some(77.5)),
        ],
    );
    let sessions = extract(&ledger, pack, 0, 240);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].direction, Direction::Discharge);
    assert!(sessions[0].energy_ac_kwh > 0.0);
    assert!((sessions[0].energy_ac_kwh - 500.0 * 120.0 / 3_600_000.0).abs() < 1e-12);
}

#[test]
fn a_one_sample_reversal_flags_both_neighbours_and_is_skipped() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, 0, 0.0, Some(50.0)),
            sample(pack, 60, CHARGE_W, Some(51.0)),
            sample(pack, 120, CHARGE_W, Some(52.0)),
            sample(pack, 180, CHARGE_W, Some(53.0)),
            sample(pack, 240, -CHARGE_W, Some(52.0)),
            sample(pack, 300, CHARGE_W, Some(53.0)),
            sample(pack, 360, CHARGE_W, Some(54.0)),
            sample(pack, 420, CHARGE_W, Some(55.0)),
            sample(pack, 480, 0.0, Some(55.5)),
        ],
    );
    let report = ledger
        .sessions()
        .unwrap()
        .extract(pack, Tier::OneMinute, 0, 540, &SessionRules::default())
        .unwrap();
    assert_eq!(report.written, 2);
    assert_eq!(report.flagged, 2);
    assert_eq!(report.skipped, 1);
    let sessions = ledger.sessions().unwrap().for_pack(pack, 0, 540).unwrap();
    assert_eq!(sessions.len(), 2);
    for session in &sessions {
        assert!(!session.complete);
        assert_eq!(
            session.invalidated_reason.as_deref(),
            Some("direction_reversal")
        );
    }
}

#[test]
fn a_soc_gap_flags_the_session_but_keeps_its_energy() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, 0, 0.0, Some(50.0)),
            sample(pack, 60, CHARGE_W, None),
            sample(pack, 120, CHARGE_W, Some(51.0)),
            sample(pack, 180, CHARGE_W, Some(52.0)),
            sample(pack, 240, 0.0, Some(52.5)),
        ],
    );
    let sessions = extract(&ledger, pack, 0, 300);
    assert_eq!(sessions.len(), 1);
    assert!(!sessions[0].complete);
    assert_eq!(sessions[0].invalidated_reason.as_deref(), Some("soc_gap"));
    assert!((sessions[0].energy_ac_kwh - CHARGE_W * 180.0 / 3_600_000.0).abs() < 1e-12);
}

#[test]
fn the_deadband_never_creates_a_session() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, 0, 10.0, Some(50.0)),
            sample(pack, 60, 15.0, Some(50.0)),
            sample(pack, 120, -10.0, Some(50.0)),
            sample(pack, 180, 0.0, Some(50.0)),
        ],
    );
    let report = ledger
        .sessions()
        .unwrap()
        .extract(pack, Tier::OneMinute, 0, 240, &SessionRules::default())
        .unwrap();
    assert_eq!(report.written, 0);
    assert_eq!(report.skipped, 0);
    assert_eq!(
        ledger
            .sessions()
            .unwrap()
            .for_pack(pack, 0, 240)
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn extraction_is_idempotent_and_rewrites_a_changed_window() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    let mut samples = vec![sample(pack, 0, 0.0, Some(50.0))];
    for step in 1..=5 {
        samples.push(sample(pack, step * 60, CHARGE_W, Some(50.0 + step as f64)));
    }
    samples.push(sample(pack, 360, 0.0, Some(55.5)));
    write(&ledger, &samples);
    let rules = SessionRules::default();
    let first = ledger
        .sessions()
        .unwrap()
        .extract(pack, Tier::OneMinute, 0, 420, &rules)
        .unwrap();
    let second = ledger
        .sessions()
        .unwrap()
        .extract(pack, Tier::OneMinute, 0, 420, &rules)
        .unwrap();
    assert_eq!(first, second);
    let before = ledger.sessions().unwrap().for_pack(pack, 0, 420).unwrap();
    assert_eq!(before.len(), 1);
    write(&ledger, &[sample(pack, 120, 3_000.0, Some(51.0))]);
    ledger
        .sessions()
        .unwrap()
        .extract(pack, Tier::OneMinute, 0, 420, &rules)
        .unwrap();
    let after = ledger.sessions().unwrap().for_pack(pack, 0, 420).unwrap();
    assert_eq!(after.len(), 1);
    let mean = (1_000.0 * 4.0 + 3_000.0) / 5.0;
    assert!((after[0].energy_ac_kwh - mean * 300.0 / 3_600_000.0).abs() < 1e-12);
}

#[test]
fn sessions_can_be_extracted_from_the_five_minute_tier() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    let samples: Vec<_> = (0..=10)
        .map(|step| sample(pack, step * 60, CHARGE_W, Some(50.0)))
        .collect();
    write(&ledger, &samples);
    ledger.measurements().unwrap().rollup(0, 600).unwrap();
    let report = ledger
        .sessions()
        .unwrap()
        .extract(pack, Tier::FiveMinute, 0, 600, &SessionRules::default())
        .unwrap();
    assert_eq!(report.written, 1);
    let rows = ledger.sessions().unwrap().for_pack(pack, 0, 600).unwrap();
    assert_eq!(rows[0].sample_count, 2);
    assert_eq!(rows[0].started_at, 0);
    assert_eq!(rows[0].ended_at, 600);
    assert!(!rows[0].complete);
    assert!((rows[0].energy_ac_kwh - CHARGE_W * 600.0 / 3_600_000.0).abs() < 1e-12);
}
