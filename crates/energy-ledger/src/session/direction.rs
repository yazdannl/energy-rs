//! Charge or discharge: the sign of the AC power relative to the deadband.

use crate::error::LedgerError;

/// Which way energy flows through the pack. Positive AC power charges the pack (into the battery),
/// negative power discharges it (into the house/grid).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Charge,
    Discharge,
}

impl Direction {
    /// Direction of a sample, or `None` when it sits inside the deadband (a hold).
    pub(crate) fn of(power_w: f64, deadband_w: f64) -> Option<Self> {
        if power_w >= deadband_w {
            Some(Self::Charge)
        } else if power_w <= -deadband_w {
            Some(Self::Discharge)
        } else {
            None
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Charge => "charge",
            Self::Discharge => "discharge",
        }
    }

    pub(crate) fn from_key(key: &str) -> Result<Self, LedgerError> {
        match key {
            "charge" => Ok(Self::Charge),
            "discharge" => Ok(Self::Discharge),
            other => Err(LedgerError::Invalid(format!(
                "session direction {other:?} is unknown"
            ))),
        }
    }
}
