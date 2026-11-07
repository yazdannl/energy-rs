//! The five PV source kinds the merge accepts.

/// One configured PV forecast source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// forecast.solar's public planes API.
    ForecastSolar,
    /// Home Assistant's own `energy/solar_forecast` providers.
    HaEnergy,
    /// The hand-tuned capacity prior (`solar.py` physics).
    Capacity,
    /// A meter-fit array size served through the capacity chain.
    Derived,
    /// The learned GBT residual on top of this crate's physics.
    Learned,
}

impl SourceKind {
    /// Parse a configured source name; `None` for anything outside the closed set.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "forecast_solar" => Some(Self::ForecastSolar),
            "ha_energy" => Some(Self::HaEnergy),
            "capacity" => Some(Self::Capacity),
            "derived" => Some(Self::Derived),
            "learned" => Some(Self::Learned),
            _ => None,
        }
    }

    /// The configured name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ForecastSolar => "forecast_solar",
            Self::HaEnergy => "ha_energy",
            Self::Capacity => "capacity",
            Self::Derived => "derived",
            Self::Learned => "learned",
        }
    }
}
