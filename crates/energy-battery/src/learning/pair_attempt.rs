//! One pairing candidate as it is published in the learning diagnostics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The charge block and the discharge block that followed it, and what they implied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairAttempt {
    /// Start of the discharge block, the instant that identifies the candidate.
    pub at: DateTime<Utc>,
    /// `accepted`, or the [`super::pair_refusal::PairRefusal`] slug.
    pub reason: String,
    /// Whether the candidate became a measurement.
    pub accepted: bool,
    /// AC out / AC in for the whole span, whether or not it was plausible.
    pub round_trip: Option<f64>,
    /// AC energy into the charge block, kWh.
    pub charge_kwh: f64,
    /// AC energy out of the discharge block, kWh.
    pub discharge_kwh: f64,
    /// SoC at the start of the charge block, percent.
    pub soc_from_pct: f64,
    /// SoC at the end of the discharge block, percent.
    pub soc_to_pct: f64,
    /// Number of contiguous charge segments in the block.
    pub charge_segments: usize,
}
