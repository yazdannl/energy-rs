//! Charge and discharge, the only two directions the device has.

use std::fmt;

use crate::report::Properties;

/// `acMode` 1 charges, `acMode` 2 discharges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Direction {
    Charge,
    Discharge,
}

impl Direction {
    pub const fn ac_mode(self) -> i32 {
        match self {
            Self::Charge => 1,
            Self::Discharge => 2,
        }
    }

    /// `None` for 0 (not reported) and for anything outside {1, 2} (a wedged device).
    pub const fn from_ac_mode(raw: i32) -> Option<Self> {
        match raw {
            1 => Some(Self::Charge),
            2 => Some(Self::Discharge),
            _ => None,
        }
    }

    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Charge => Self::Discharge,
            Self::Discharge => Self::Charge,
        }
    }

    /// The magnitude limit that is live for this direction.
    pub fn limit(self, properties: &Properties) -> u32 {
        match self {
            Self::Charge => properties.input_limit_w,
            Self::Discharge => properties.output_limit_w,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Charge => "charge",
            Self::Discharge => "discharge",
        }
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}
