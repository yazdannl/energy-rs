//! A validated, model-specific hard ceiling.

use crate::control::direction::Direction;
use crate::error::BatteryError;

/// No zenSDK device class sits above this: a hard ceiling beyond it is a configuration typo, not a
/// device. The settings schema caps the operator's power limit at the same value.
pub const MAX_HARD_LIMIT_W: u32 = 4_000;

/// The last-resort ceiling for one device model.
///
/// A device may report `chargeMaxLimit`/`inverseMaxPower` as 0 ("no value"), and the operator may
/// leave the config ceiling unset. Without a third, trusted number a nonzero command has no bound at
/// all, so this is the one the control plane falls back to — and it only exists when it was
/// validated: both directions nonzero and at or below [`MAX_HARD_LIMIT_W`]. A wrong ceiling is not a
/// safety net, so construction refuses rather than guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardLimit {
    charge: u32,
    discharge: u32,
}

impl HardLimit {
    /// Validate the two bounds, one per direction.
    ///
    /// # Errors
    ///
    /// [`BatteryError::Invalid`] when a direction is 0 (that would forbid all power) or above
    /// [`MAX_HARD_LIMIT_W`] (that would bound nothing this hardware can do).
    pub fn new(charge: u32, discharge: u32) -> Result<Self, BatteryError> {
        let bounds = [
            (Direction::Charge, charge),
            (Direction::Discharge, discharge),
        ];
        for (direction, watts) in bounds {
            if watts == 0 || watts > MAX_HARD_LIMIT_W {
                return Err(BatteryError::Invalid {
                    reason: format!(
                        "hard {direction} ceiling {watts} W is outside 1..={MAX_HARD_LIMIT_W} W"
                    ),
                });
            }
        }
        Ok(Self { charge, discharge })
    }

    pub const fn for_direction(self, direction: Direction) -> u32 {
        match direction {
            Direction::Charge => self.charge,
            Direction::Discharge => self.discharge,
        }
    }
}
