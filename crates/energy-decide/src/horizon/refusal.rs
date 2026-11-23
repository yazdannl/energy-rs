//! Every M1 refusal is typed, carries its evidence and permits safety only.

use thiserror::Error;

use crate::horizon::EmptyReason;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum HorizonRefusal {
    #[error("horizon of {slots} slots is outside 1..=120")]
    HorizonSlots { slots: usize },
    #[error("slot at {t_start} has {got_s}s rows; {expected_s}s required")]
    PriceResolution {
        expected_s: i64,
        got_s: i64,
        t_start: i64,
    },
    #[error("price gap at {first_missing_t_start} for {missing_count} slot(s)")]
    PriceGap {
        first_missing_t_start: i64,
        missing_count: usize,
    },
    #[error("prices cover {actual} slots; {requested} requested, {minimum} required")]
    InsufficientCoverage {
        actual: usize,
        requested: usize,
        minimum: usize,
    },
    #[error("prices are {age_s}s old, limit {limit_s}s")]
    StalePrices { age_s: i64, limit_s: i64 },
    #[error("required input unavailable: {source_id}, {missing_slots} slot(s) missing")]
    InputsUnavailable {
        missing_slots: usize,
        source_id: String,
        empty_reason: Option<EmptyReason>,
    },
    #[error("no fresh forecast: {reason}")]
    ForecastUnavailable { reason: &'static str },
    #[error("unknown net-metering regime; raw value redacted")]
    UnknownRegime,
    #[error("explicit net-metering override requires acknowledgement, expiry and event")]
    OverrideMissing,
    #[error("explicit net-metering override expired at {expires_at}")]
    OverrideExpired { expires_at: i64 },
    #[error("operator acknowledgement is missing accepted_by or event")]
    OverrideIncomplete,
    #[error("operator acknowledgement names a different regime")]
    OverrideMismatch,
    #[error("slot {t_start}..{t_end} crosses the net-metering boundary {boundary}")]
    RegimeBoundaryCrossing {
        t_start: i64,
        t_end: i64,
        boundary: i64,
    },
    #[error("slot start {t_start} is not a representable timestamp")]
    InvalidSlotTime { t_start: i64 },
}

impl HorizonRefusal {
    /// Stable reason slug that reaches the assembled plan.
    pub const fn reason_slug(&self) -> &'static str {
        match self {
            Self::HorizonSlots { .. } => "HorizonSlots",
            Self::PriceResolution { .. } => "PriceResolution",
            Self::PriceGap { .. } => "PriceGap",
            Self::InsufficientCoverage { .. } => "InsufficientCoverage",
            Self::StalePrices { .. } => "StalePrices",
            Self::InputsUnavailable { .. } => "InputsUnavailable",
            Self::ForecastUnavailable { .. } => "ForecastUnavailable",
            Self::UnknownRegime => "UnknownRegime",
            Self::OverrideMissing => "OverrideMissing",
            Self::OverrideExpired { .. } => "OverrideExpired",
            Self::OverrideIncomplete => "OverrideIncomplete",
            Self::OverrideMismatch => "OverrideMismatch",
            Self::RegimeBoundaryCrossing { .. } => "RegimeBoundaryCrossing",
            Self::InvalidSlotTime { .. } => "InvalidSlotTime",
        }
    }
}
