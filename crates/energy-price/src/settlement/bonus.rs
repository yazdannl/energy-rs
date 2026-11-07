//! The Zonnebonus as data: a multiplier with four guards (PV only, daylight, positive feed-in,
//! annual kWh cap).  `Contract` applies the guards; this type only holds the constants.

use crate::error::PriceError;

/// Zonnebonus rules.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BonusRules {
    /// Extra fraction on the post-saldering feed-in price (0.10 = 10 %).
    pub pct: f64,
    /// Annual export cap that earns the bonus, kWh.
    pub cap_kwh: f64,
    /// Only PV export earns the bonus (never home-battery export).
    pub pv_only: bool,
    /// Only sunrise-to-sunset export earns the bonus.
    pub daylight: bool,
}

impl BonusRules {
    /// No bonus (other suppliers, and the 2026 netted regime).
    pub const fn none() -> Self {
        Self {
            pct: 0.0,
            cap_kwh: 0.0,
            pv_only: true,
            daylight: true,
        }
    }

    /// Zonneplan Zonnebonus 2027+: 10 %, 7 500 kWh/yr, PV-only, daylight-only.
    pub const fn zonnebonus() -> Self {
        Self {
            pct: 0.10,
            cap_kwh: 7500.0,
            pv_only: true,
            daylight: true,
        }
    }

    /// Refuse rules that cannot be applied.
    pub fn validate(&self) -> Result<(), PriceError> {
        if !self.pct.is_finite() || !(0.0..=10.0).contains(&self.pct) {
            return Err(PriceError::Contract {
                field: "bonus_pct",
                value: self.pct,
            });
        }
        if !self.cap_kwh.is_finite() || self.cap_kwh < 0.0 {
            return Err(PriceError::Contract {
                field: "bonus_cap_kwh",
                value: self.cap_kwh,
            });
        }
        Ok(())
    }
}
