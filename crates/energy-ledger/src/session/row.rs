//! A stored charge or discharge session.

use crate::session::Direction;

/// One extracted session.
///
/// `energy_ac_kwh` is the measured AC energy magnitude: `|mean power|` of the run times the interval
/// its samples actually cover; [`direction`](Session::direction) says which way it flowed. `soc_start`/`soc_end` are the first and last state of charge of the run,
/// or `None` when the pack did not report it. `complete` is true only when a hold or opposite
/// sample bounds the run on both sides and no SoC gap occurred; `invalidated_reason` then records
/// why a flagged run must not be used for learning (`soc_gap`, `direction_reversal`).
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: i64,
    pub pack_id: i64,
    pub direction: Direction,
    pub started_at: i64,
    pub ended_at: i64,
    pub soc_start: Option<f64>,
    pub soc_end: Option<f64>,
    pub energy_ac_kwh: f64,
    pub duration_s: i64,
    pub sample_count: i64,
    pub complete: bool,
    pub invalidated_reason: Option<String>,
}

impl Session {
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let direction_key: String = row.get(2)?;
        let direction = Direction::from_key(&direction_key).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(err))
        })?;
        let complete: i64 = row.get(10)?;
        Ok(Self {
            id: row.get(0)?,
            pack_id: row.get(1)?,
            direction,
            started_at: row.get(3)?,
            ended_at: row.get(4)?,
            soc_start: row.get(5)?,
            soc_end: row.get(6)?,
            energy_ac_kwh: row.get(7)?,
            duration_s: row.get(8)?,
            sample_count: row.get(9)?,
            complete: complete != 0,
            invalidated_reason: row.get(11)?,
        })
    }
}
