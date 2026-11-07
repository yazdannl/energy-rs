//! Learning fixtures: deterministic sessions with a known capacity and round trip.
//!
//! The energy of a synthetic leg follows the same two formulas the learner inverts, so a test can
//! assert the exact number back instead of a tolerance band: charging takes `dSOC · C / x` kWh from
//! the AC side, discharging delivers `dSOC · C · x`.

use chrono::{DateTime, Duration, TimeZone, Utc};
use energy_battery::{CapacityBounds, Direction, SessionRecord};

/// Pack serial used by every learning fixture.
pub const PACK: &str = "CO4A000000000001";
/// The labelled AB2000 capacity, kWh.
pub const LABEL_KWH: f64 = 1.92;
/// The per-leg efficiency used by the fixtures (`round trip = 0.81`).
pub const SPLIT: f64 = 0.9;

/// A fixed epoch: nothing in these tests ever reads a clock.
pub fn base() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
}

/// `base()` plus `hours`, the usual way to order fixtures.
pub fn at(hours: i64) -> DateTime<Utc> {
    base() + Duration::hours(hours)
}

/// The pack's publishing bounds.
pub fn bounds(reported_kwh: Option<f64>) -> CapacityBounds {
    CapacityBounds::new(LABEL_KWH, reported_kwh)
}

/// One valid session record with the given numbers and a one-hour duration.
pub fn session(
    direction: Direction,
    soc_start_pct: f64,
    soc_end_pct: f64,
    energy_ac_kwh: f64,
    started_at: DateTime<Utc>,
) -> SessionRecord {
    SessionRecord {
        pack_id: PACK.to_owned(),
        direction,
        started_at,
        ended_at: started_at + Duration::hours(1),
        soc_start_pct,
        soc_end_pct,
        energy_ac_kwh,
        duration_s: 3600.0,
        sample_count: 60,
        complete: true,
    }
}

/// A charge leg moving `from` → `to` percent on a pack of `capacity` kWh with split `split`.
pub fn charge(
    from: f64,
    to: f64,
    capacity: f64,
    split: f64,
    started_at: DateTime<Utc>,
) -> SessionRecord {
    let energy = (to - from) / 100.0 * capacity / split;
    session(Direction::Charge, from, to, energy, started_at)
}

/// A discharge leg moving `from` → `to` (downwards) on the same pack.
pub fn discharge(
    from: f64,
    to: f64,
    capacity: f64,
    split: f64,
    started_at: DateTime<Utc>,
) -> SessionRecord {
    let energy = (from - to) / 100.0 * capacity * split;
    session(Direction::Discharge, from, to, energy, started_at)
}

/// A complete charge → discharge cycle that closes back on its start SoC.
pub fn cycle(
    from: f64,
    to: f64,
    capacity: f64,
    split: f64,
    started_at: DateTime<Utc>,
) -> Vec<SessionRecord> {
    vec![
        charge(from, to, capacity, split, started_at),
        discharge(to, from, capacity, split, started_at + Duration::hours(2)),
    ]
}

/// `count` closed cycles on the same span, one every four hours.
pub fn cycles(from: f64, to: f64, capacity: f64, split: f64, count: usize) -> Vec<SessionRecord> {
    let mut sessions = Vec::new();
    for index in 0..count {
        sessions.extend(cycle(from, to, capacity, split, at(index as i64 * 4)));
    }
    sessions
}

/// `count` charge-only sessions on the same span, one every two hours.
pub fn charges(from: f64, to: f64, capacity: f64, split: f64, count: usize) -> Vec<SessionRecord> {
    (0..count)
        .map(|index| charge(from, to, capacity, split, at(index as i64 * 2)))
        .collect()
}

/// `count` discharge-only sessions on the same span, one every two hours.
pub fn discharges(
    from: f64,
    to: f64,
    capacity: f64,
    split: f64,
    count: usize,
) -> Vec<SessionRecord> {
    (0..count)
        .map(|index| discharge(from, to, capacity, split, at(index as i64 * 2)))
        .collect()
}
