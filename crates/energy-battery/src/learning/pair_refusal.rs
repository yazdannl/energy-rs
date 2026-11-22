//! Why a charge → discharge span could not measure a round trip.

use chrono::{DateTime, Utc};

/// One refused pairing candidate, with the timestamp that identifies its discharge.
#[derive(Debug, Clone, PartialEq)]
pub enum PairRefusal {
    /// The window starts inside a discharge: there is no charge leg to compare it with.
    NoChargeBefore { at: DateTime<Utc> },
    /// A charge and a discharge overlap in time; they are not two legs of one cycle.
    OverlappingLegs { at: DateTime<Utc> },
    /// More than a week of idle time between the legs.
    GapTooLong { at: DateTime<Utc>, days: f64 },
    /// The pack's SoC moved between the end of the charge and the start of the discharge.
    HoldDrift { at: DateTime<Utc>, drift_pct: f64 },
    /// The discharge did not bring the pack back to where the charge block started.
    NotClosed { at: DateTime<Utc>, end_pct: f64 },
    /// One leg moved no energy onto the AC side.
    NoEnergy { at: DateTime<Utc> },
    /// An AC round trip outside `[MIN_ROUND_TRIP, MAX_ROUND_TRIP]`.
    ImplausibleRoundTrip { at: DateTime<Utc>, round_trip: f64 },
    /// The energy and SoC spans imply a capacity no AB2000 can have.
    ImplausibleCapacity {
        at: DateTime<Utc>,
        capacity_kwh: f64,
    },
    /// A candidate carried a non-finite measurement.
    NotFinite { at: DateTime<Utc> },
    /// A leg existed as a direction change but failed its own session validity checks.
    UnusableLeg {
        at: DateTime<Utc>,
        reason: &'static str,
    },
}

impl PairRefusal {
    /// A stable slug for logs and the UI, never a sentence built at runtime.
    pub const fn slug(&self) -> &'static str {
        match self {
            Self::NoChargeBefore { .. } => "no_charge_before",
            Self::OverlappingLegs { .. } => "overlapping_legs",
            Self::GapTooLong { .. } => "gap_too_long",
            Self::HoldDrift { .. } => "hold_drift",
            Self::NotClosed { .. } => "not_closed",
            Self::NoEnergy { .. } => "no_energy",
            Self::ImplausibleRoundTrip { .. } => "implausible_round_trip",
            Self::ImplausibleCapacity { .. } => "implausible_capacity",
            Self::NotFinite { .. } => "not_finite",
            Self::UnusableLeg { reason, .. } => reason,
        }
    }
}
