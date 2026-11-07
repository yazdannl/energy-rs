//! Why a session could not teach anything.

use chrono::{DateTime, Utc};

/// One measurement the learner refused, with the timestamp that identifies it.
///
/// The report carries these instead of silently dropping rows: "no estimate" and "a 120-point
/// direction reversal" are different failures, and the caller may log or surface them differently.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// The ledger marked the run incomplete (the add-on restarted inside it, or a sample was lost).
    Incomplete { at: DateTime<Utc> },
    /// Fewer SoC samples than a capacity estimate can be trusted on.
    ShortSession { at: DateTime<Utc>, samples: u32 },
    /// A NaN or infinite value — never a number a pack measured.
    NotFinite { at: DateTime<Utc> },
    /// No AC energy, a non-positive duration, or a run that ends before it starts.
    EmptyMeasurement { at: DateTime<Utc> },
    /// The SoC moved against the declared direction: a charge that ended lower than it started.
    DirectionReversal { at: DateTime<Utc>, span_pct: f64 },
    /// A session for another pack was handed to this pack's learner.
    ForeignPack { at: DateTime<Utc>, pack_id: String },
}

impl Refusal {
    /// The instant the refused measurement belongs to.
    pub const fn at(&self) -> DateTime<Utc> {
        match self {
            Self::Incomplete { at }
            | Self::ShortSession { at, .. }
            | Self::NotFinite { at }
            | Self::EmptyMeasurement { at }
            | Self::DirectionReversal { at, .. }
            | Self::ForeignPack { at, .. } => *at,
        }
    }

    /// A stable slug for logs and the UI, never a sentence built at runtime.
    pub const fn slug(&self) -> &'static str {
        match self {
            Self::Incomplete { .. } => "session_incomplete",
            Self::ShortSession { .. } => "session_short",
            Self::NotFinite { .. } => "session_not_finite",
            Self::EmptyMeasurement { .. } => "session_empty",
            Self::DirectionReversal { .. } => "session_direction_reversal",
            Self::ForeignPack { .. } => "session_foreign_pack",
        }
    }
}
