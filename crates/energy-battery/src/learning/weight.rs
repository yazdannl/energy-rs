//! Recency weighting of learned samples.

use chrono::{DateTime, Utc};

use crate::control::Direction;

/// Sessions after which a sample's weight has halved.
///
/// Three sessions is roughly a day of cycling on the measured fleet: the last three runs describe
/// the pack as it is now, while a measurement from a fortnight ago (an aged pack, a firmware change)
/// should bend the estimate, not set it.
pub const HALF_LIFE_SESSIONS: f64 = 3.0;

/// The weight of the `rank`-th newest session (`rank` 0 is the newest and weighs 1).
pub fn recency_weight(rank: usize) -> f64 {
    0.5_f64.powf(rank as f64 / HALF_LIFE_SESSIONS)
}

/// One session as the capacity estimate saw it, kept in the report as provenance.
///
/// `weight` is the recency weight times nothing else: the MAD filter and the plausibility envelope
/// decide `kept`, so the report can explain why a session that was read still did not count.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionWeight {
    /// The run's first sample.
    pub started_at: DateTime<Utc>,
    /// The AC direction of the run.
    pub direction: Direction,
    /// Position when sorted newest first, 0-based.
    pub rank: usize,
    /// Recency weight in `(0, 1]`.
    pub weight: f64,
    /// The capacity this one session implies, kWh (before any band clamp).
    pub capacity_kwh: f64,
    /// True when the session was used as the discharge fallback rather than the charge basis.
    pub fallback: bool,
    /// False when the plausibility envelope or the MAD filter dropped it.
    pub kept: bool,
}
