//! The four commands this client can send.

use crate::control::direction::Direction;
use crate::transport::WriteProperties;

/// What to do with one device.
///
/// Charge and discharge are direction changes (the other limit is zeroed in the same body). `Hold`
/// is AC at 0 W with RAM retention and no `acMode`. `Trim` changes only the running direction's
/// limit; `smartMode: 1` is still re-asserted because the device resets it to 0 on every reboot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Charge { watts: u32 },
    Discharge { watts: u32 },
    Hold,
    Trim { direction: Direction, watts: u32 },
}

impl Command {
    /// The direction this command drives, `None` for a hold.
    pub const fn direction(&self) -> Option<Direction> {
        match self {
            Self::Charge { .. } => Some(Direction::Charge),
            Self::Discharge { .. } => Some(Direction::Discharge),
            Self::Hold => None,
            Self::Trim { direction, .. } => Some(*direction),
        }
    }

    /// The magnitude this command asks for, `None` for a hold.
    pub const fn watts(&self) -> Option<u32> {
        match self {
            Self::Charge { watts } | Self::Discharge { watts } | Self::Trim { watts, .. } => {
                Some(*watts)
            }
            Self::Hold => None,
        }
    }

    pub const fn is_hold(&self) -> bool {
        matches!(self, Self::Hold)
    }

    /// Replace the magnitude (used by clamping), keeping the command kind.
    #[must_use]
    pub const fn with_watts(self, watts: u32) -> Self {
        match self {
            Self::Charge { .. } => Self::Charge { watts },
            Self::Discharge { .. } => Self::Discharge { watts },
            Self::Hold => Self::Hold,
            Self::Trim { direction, .. } => Self::Trim { direction, watts },
        }
    }

    /// The exact property set this command puts on the wire.
    pub fn properties(&self) -> WriteProperties {
        match *self {
            Self::Charge { watts } => WriteProperties::charge(watts),
            Self::Discharge { watts } => WriteProperties::discharge(watts),
            Self::Hold => WriteProperties::hold(),
            Self::Trim { direction, watts } => match direction {
                Direction::Charge => WriteProperties::trim_charge(watts),
                Direction::Discharge => WriteProperties::trim_discharge(watts),
            },
        }
    }
}
