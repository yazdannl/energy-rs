//! Per-slot net-metering regime in the authoritative tariff timezone.

use crate::horizon::{
    AcknowledgedRegime, HorizonRefusal, OperatorOverride, SettlementReason, local_year,
};

/// The first local year without net metering (2027-01-01 local, inclusive).
pub const NETTING_END_YEAR: i32 = 2027;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetMeteringRegime {
    /// Date-driven default: active while the slot's Amsterdam date is before 2027.
    DateDriven,
    /// Explicit on override with the complete operator contract.
    On(OperatorOverride),
    /// Explicit off override with the complete operator contract.
    Off(OperatorOverride),
}

impl NetMeteringRegime {
    /// Parse a stored value; unknown values never default to on, and the raw value is not retained.
    pub fn from_config(
        raw: &str,
        now: i64,
        operator: Option<OperatorOverride>,
    ) -> Result<Self, HorizonRefusal> {
        if raw.eq_ignore_ascii_case("date_driven") || raw.eq_ignore_ascii_case("date-driven") {
            return Ok(Self::DateDriven);
        }
        let expected = if raw.eq_ignore_ascii_case("on") {
            AcknowledgedRegime::On
        } else if raw.eq_ignore_ascii_case("off") {
            AcknowledgedRegime::Off
        } else {
            return Err(HorizonRefusal::UnknownRegime);
        };
        let operator = operator.ok_or(HorizonRefusal::OverrideMissing)?;
        validate_operator(&operator, expected, now)?;
        Ok(match expected {
            AcknowledgedRegime::On => Self::On(operator),
            AcknowledgedRegime::Off => Self::Off(operator),
        })
    }

    /// Whether one slot (never a straddling window) is netted.
    pub fn is_netted(&self, t_start: i64, now: i64) -> Result<bool, HorizonRefusal> {
        match self {
            Self::On(operator) => {
                check_expiry(now, operator.expires_at)?;
                Ok(true)
            }
            Self::Off(operator) => {
                check_expiry(now, operator.expires_at)?;
                Ok(false)
            }
            Self::DateDriven => local_year(t_start)
                .map(|year| year < NETTING_END_YEAR)
                .ok_or(HorizonRefusal::InvalidSlotTime { t_start }),
        }
    }

    /// Reason used when a slot is priced outside the netting regime.
    pub const fn inactive_reason(&self) -> SettlementReason {
        SettlementReason::NetMeteringInactive
    }
}

fn validate_operator(
    operator: &OperatorOverride,
    expected: AcknowledgedRegime,
    now: i64,
) -> Result<(), HorizonRefusal> {
    if operator.regime != expected {
        return Err(HorizonRefusal::OverrideMismatch);
    }
    if operator.accepted_by.trim().is_empty() || operator.event.trim().is_empty() {
        return Err(HorizonRefusal::OverrideIncomplete);
    }
    check_expiry(now, operator.expires_at)
}

fn check_expiry(now: i64, expires_at: i64) -> Result<(), HorizonRefusal> {
    if now >= expires_at {
        return Err(HorizonRefusal::OverrideExpired { expires_at });
    }
    Ok(())
}
