//! The learning input: one contiguous same-direction run of measurements.

use chrono::{DateTime, Utc};

use crate::control::Direction;
use crate::learning::refusal::Refusal;

/// Minimum SoC samples a session must carry before it may teach anything.
///
/// The ledger samples at the dispatch tick; a run shorter than twenty points is a dispatch
/// hiccup, not a capacity measurement, and letting it in would make the estimate jitter.
pub const MIN_SESSION_SAMPLES: u32 = 20;

/// One pack's contiguous charge or discharge run, as the ledger recorded it.
///
/// This is a plain DTO on purpose: `energy-ledger` owns the rows and the learner must not depend on
/// its storage layer, so the engine maps rows into these records and hands them over.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    /// Serial (or stable ledger id) of the pack the run belongs to.
    pub pack_id: String,
    /// The AC direction of the whole run.
    pub direction: Direction,
    /// First sample of the run.
    pub started_at: DateTime<Utc>,
    /// Last sample of the run.
    pub ended_at: DateTime<Utc>,
    /// Reported SoC at the first sample, percent.
    pub soc_start_pct: f64,
    /// Reported SoC at the last sample, percent.
    pub soc_end_pct: f64,
    /// AC energy the run moved, kWh (already the measured integral, never a setpoint).
    pub energy_ac_kwh: f64,
    /// Wall-clock duration of the run, seconds.
    pub duration_s: f64,
    /// Number of SoC samples in the run.
    pub sample_count: u32,
    /// Whether the run closed on a direction change (a reopened run is not a measurement).
    pub complete: bool,
}

impl SessionRecord {
    /// The SoC span the run moved in its own direction, in points (positive for a valid run).
    pub fn soc_span_pct(&self) -> f64 {
        match self.direction {
            Direction::Charge => self.soc_end_pct - self.soc_start_pct,
            Direction::Discharge => self.soc_start_pct - self.soc_end_pct,
        }
    }

    /// Whether every numeric field is a real number.
    pub fn is_finite(&self) -> bool {
        self.soc_start_pct.is_finite()
            && self.soc_end_pct.is_finite()
            && self.energy_ac_kwh.is_finite()
            && self.duration_s.is_finite()
            && self.soc_span_pct().is_finite()
    }

    /// Why this session cannot teach anything, or `None` when it can.
    ///
    /// The checks are ordered so the most specific reason wins: an incomplete run is reported as
    /// incomplete even when its sample count is fine, and a NaN span is a non-finite value before it
    /// is a direction reversal.
    pub fn refusal(&self) -> Option<Refusal> {
        let at = self.started_at;
        if !self.complete {
            return Some(Refusal::Incomplete { at });
        }
        if self.sample_count < MIN_SESSION_SAMPLES {
            return Some(Refusal::ShortSession {
                at,
                samples: self.sample_count,
            });
        }
        if !self.is_finite() {
            return Some(Refusal::NotFinite { at });
        }
        if self.energy_ac_kwh <= 0.0 || self.duration_s <= 0.0 || self.ended_at <= self.started_at {
            return Some(Refusal::EmptyMeasurement { at });
        }
        let span = self.soc_span_pct();
        if span <= 0.0 {
            return Some(Refusal::DirectionReversal { at, span_pct: span });
        }
        None
    }
}
