//! The closed device-kind set, and which kinds get a temperature term.

/// A `load.devices[].kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceKind {
    /// Air/water heat pump.
    HeatPump,
    /// EV charger.
    EvCharger,
    /// Domestic hot-water heater.
    WaterHeater,
    /// Swimming-pool pump.
    PoolPump,
    /// Any other string, kept as a plain meter with no temperature term.
    Unknown(String),
}

impl DeviceKind {
    /// Parse a configured kind string (already trimmed and lowercased).
    pub fn parse(text: &str) -> Self {
        match text {
            "heat_pump" => Self::HeatPump,
            "ev_charger" => Self::EvCharger,
            "water_heater" => Self::WaterHeater,
            "pool_pump" => Self::PoolPump,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// The configured string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::HeatPump => "heat_pump",
            Self::EvCharger => "ev_charger",
            Self::WaterHeater => "water_heater",
            Self::PoolPump => "pool_pump",
            Self::Unknown(other) => other,
        }
    }

    /// Whether the device's draw is expected to respond to outdoor temperature.
    ///
    /// An EV charger is deliberately absent: its draw is a schedule, and a slope fitted to it would
    /// quietly absorb whatever else varies with temperature.
    pub fn is_temperature(&self) -> bool {
        matches!(self, Self::HeatPump | Self::WaterHeater | Self::PoolPump)
    }

    /// Default display name, or `None` for an unknown kind.
    pub fn label(&self) -> Option<&'static str> {
        match self {
            Self::HeatPump => Some("Heat pump"),
            Self::EvCharger => Some("EV charger"),
            Self::WaterHeater => Some("Water heater"),
            Self::PoolPump => Some("Pool pump"),
            Self::Unknown(_) => None,
        }
    }
}

impl std::fmt::Display for DeviceKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
