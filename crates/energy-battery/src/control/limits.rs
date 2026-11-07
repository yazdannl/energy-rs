//! The operator's power ceilings.

use crate::control::direction::Direction;
use crate::control::hard_limit::HardLimit;

/// Per-device ceiling from configuration, in watts.
///
/// A 0 means "no config ceiling": the device's own `chargeMaxLimit`/`inverseMaxPower` then governs.
/// On the measured fleet those are 1000 W and 800 W, but they are read live, never assumed.
///
/// `hard` is the model's validated fallback for a device that reports neither (see [`HardLimit`]).
/// With no config ceiling, no hard ceiling and no live maximum there is no bound at all, and the
/// control plane refuses to send a nonzero command rather than treat that as unlimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControlLimits {
    pub charge: u32,
    pub discharge: u32,
    pub hard: Option<HardLimit>,
}

impl ControlLimits {
    pub const fn new(charge: u32, discharge: u32) -> Self {
        Self {
            charge,
            discharge,
            hard: None,
        }
    }

    /// No config ceiling at all; the device decides.
    pub const fn unlimited() -> Self {
        Self::new(0, 0)
    }

    /// Attach the model's validated hard ceiling, the fallback when the device reports nothing.
    #[must_use]
    pub const fn with_hard(mut self, hard: Option<HardLimit>) -> Self {
        self.hard = hard;
        self
    }

    pub const fn for_direction(self, direction: Direction) -> u32 {
        match direction {
            Direction::Charge => self.charge,
            Direction::Discharge => self.discharge,
        }
    }
}
