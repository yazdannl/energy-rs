//! The operator's power ceilings.

use crate::control::direction::Direction;

/// Per-device ceiling from configuration, in watts.
///
/// A 0 means "no config ceiling": the device's own `chargeMaxLimit`/`inverseMaxPower` then governs.
/// On the measured fleet those are 1000 W and 800 W, but they are read live, never assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControlLimits {
    pub charge: u32,
    pub discharge: u32,
}

impl ControlLimits {
    pub const fn new(charge: u32, discharge: u32) -> Self {
        Self { charge, discharge }
    }

    /// No config ceiling at all; the device decides.
    pub const fn unlimited() -> Self {
        Self {
            charge: 0,
            discharge: 0,
        }
    }

    pub const fn for_direction(self, direction: Direction) -> u32 {
        match direction {
            Direction::Charge => self.charge,
            Direction::Discharge => self.discharge,
        }
    }
}
