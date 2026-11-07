//! Where a slot came from.  The settlement grid is 15 minutes, but a provider may only have the
//! legacy hourly series; the source travels with every slot so the planner can see the resolution.

use std::fmt;
use std::str::FromStr;

use crate::error::PriceError;

/// The provider and resolution that produced a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotSource {
    /// Zonneplan app API, `electricity-quarter-hourly`.
    ZonneplanQuarterHourly,
    /// Zonneplan app API, legacy `electricity-hourly` fallback.
    ZonneplanHourly,
    /// HA integration sensor forecast, quarter-hourly shape.
    HaQuarterHourly,
    /// HA integration sensor forecast, legacy hourly shape.
    HaHourly,
}

impl SlotSource {
    /// Slot duration in seconds.
    pub const fn slot_seconds(self) -> i64 {
        match self {
            SlotSource::ZonneplanQuarterHourly | SlotSource::HaQuarterHourly => 900,
            SlotSource::ZonneplanHourly | SlotSource::HaHourly => 3600,
        }
    }

    /// Whether this source is the 60-minute legacy view.
    pub const fn is_hourly(self) -> bool {
        self.slot_seconds() == 3600
    }

    /// Stable lowercase key used by the store.
    pub const fn as_str(self) -> &'static str {
        match self {
            SlotSource::ZonneplanQuarterHourly => "zonneplan_quarter_hourly",
            SlotSource::ZonneplanHourly => "zonneplan_hourly",
            SlotSource::HaQuarterHourly => "ha_quarter_hourly",
            SlotSource::HaHourly => "ha_hourly",
        }
    }

    /// Parse the store key.  Unknown keys are refused rather than mapped to a default.
    pub fn from_key(key: &str) -> Result<Self, PriceError> {
        match key {
            "zonneplan_quarter_hourly" => Ok(SlotSource::ZonneplanQuarterHourly),
            "zonneplan_hourly" => Ok(SlotSource::ZonneplanHourly),
            "ha_quarter_hourly" => Ok(SlotSource::HaQuarterHourly),
            "ha_hourly" => Ok(SlotSource::HaHourly),
            other => Err(PriceError::Payload(format!("unknown slot source {other}"))),
        }
    }
}

impl fmt::Display for SlotSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SlotSource {
    type Err = PriceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SlotSource::from_key(s)
    }
}
