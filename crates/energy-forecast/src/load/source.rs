//! Where a served load forecast came from.

/// The source tier a load forecast was served from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadSource {
    /// A learned profile.
    Profile,
    /// The flat default shape scaled to `load.base_kwh_per_day`.
    Fallback,
    /// The flat shape at zero, because no base level is configured either.
    FallbackEmpty,
}

impl LoadSource {
    /// Classify: a profile wins, otherwise the fallback (empty when the base level is zero).
    pub fn classify(has_profile: bool, base_kwh_per_day: f64) -> Self {
        if has_profile {
            Self::Profile
        } else if base_kwh_per_day <= 0.0 {
            Self::FallbackEmpty
        } else {
            Self::Fallback
        }
    }

    /// The status string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::Fallback => "fallback",
            Self::FallbackEmpty => "fallback-empty",
        }
    }
}
