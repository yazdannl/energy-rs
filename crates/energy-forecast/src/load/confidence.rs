//! Confidence class of a served load forecast.

use super::bucket::MIN_SAMPLES;
use super::profile::LoadProfile;

/// How much evidence stands behind a load forecast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// No profile, or fewer than 48 hourly samples.
    Low,
    /// A profile covering fewer than 14 distinct days.
    Medium,
    /// A profile with at least 48 samples over at least 14 days.
    High,
}

impl Confidence {
    /// Classify a profile, or the absence of one.
    pub fn from_profile(profile: Option<&LoadProfile>) -> Self {
        let Some(profile) = profile else {
            return Self::Low;
        };
        if profile.samples < MIN_SAMPLES * 2 {
            return Self::Low;
        }
        if profile.days < 14 {
            return Self::Medium;
        }
        Self::High
    }

    /// The status string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}
