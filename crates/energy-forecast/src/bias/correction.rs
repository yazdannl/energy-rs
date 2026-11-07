//! The correction applied once, outside the model: EWMA by default, head when promoted.

use chrono::{DateTime, Utc};

use super::head_profile::HeadProfile;
use super::hourly::HourlyBias;

/// Which correction is in force.
#[derive(Debug, Clone)]
pub struct Correction {
    source: CorrectionSource,
}

/// The implementations behind [`Correction`]. A promoted head **replaces** the EWMA; they are
/// never multiplied together, because both are estimates of the same multiplicative error.
#[derive(Debug, Clone)]
enum CorrectionSource {
    Neutral,
    Hourly(HourlyBias),
    Head(HeadProfile),
}

impl Default for Correction {
    fn default() -> Self {
        Self::neutral()
    }
}

impl Correction {
    /// No correction: every factor is 1.0.
    pub fn neutral() -> Self {
        Self {
            source: CorrectionSource::Neutral,
        }
    }

    /// The shipped EWMA correction.
    pub fn hourly(bias: HourlyBias) -> Self {
        Self {
            source: CorrectionSource::Hourly(bias),
        }
    }

    /// Select the correction from the two learned profiles: a promoted head wins outright.
    pub fn from_profiles(head: Option<HeadProfile>, bias: HourlyBias) -> Self {
        match head {
            Some(profile) if profile.promoted => Self {
                source: CorrectionSource::Head(profile),
            },
            _ => Self::hourly(bias),
        }
    }

    /// The factor for one local hour at `now`.
    pub fn factor(&self, local_hour: u32, now: DateTime<Utc>) -> f64 {
        let hour = (local_hour % 24) as usize;
        match &self.source {
            CorrectionSource::Neutral => 1.0,
            CorrectionSource::Hourly(bias) => bias.factors_at(now)[hour],
            CorrectionSource::Head(head) => {
                head.active_factors().map_or(1.0, |factors| factors[hour])
            }
        }
    }

    /// Apply the correction to a kW value, floored at zero as the provider does.
    pub fn apply(&self, pv_kw: f64, local_hour: u32, now: DateTime<Utc>) -> f64 {
        (pv_kw * self.factor(local_hour, now)).max(0.0)
    }

    /// `none`, `ewma` or `head`, for the status detail.
    pub fn kind(&self) -> &'static str {
        match &self.source {
            CorrectionSource::Neutral => "none",
            CorrectionSource::Hourly(_) => "ewma",
            CorrectionSource::Head(_) => "head",
        }
    }

    /// Whether every factor is 1.0 by construction.
    pub fn is_neutral(&self) -> bool {
        matches!(self.source, CorrectionSource::Neutral)
    }
}
