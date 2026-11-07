//! The device-side SoC bound state.

/// `socLimit`: the direction the device has closed on its own.
///
/// At `Upper` a charge command is ignored and the flow stays at 0 W, which otherwise looks like a
/// dropped write, so the controller must not command into it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SocLimit {
    /// No bound active.
    #[default]
    None,
    /// Upper bound reached: charging is closed.
    Upper,
    /// Lower bound reached: discharging is closed.
    Lower,
    /// A value no reference implementation documents.
    Other(i32),
}

impl SocLimit {
    pub const fn from_raw(raw: i32) -> Self {
        match raw {
            0 => Self::None,
            1 => Self::Upper,
            2 => Self::Lower,
            other => Self::Other(other),
        }
    }

    pub const fn blocks_charge(self) -> bool {
        matches!(self, Self::Upper)
    }

    pub const fn blocks_discharge(self) -> bool {
        matches!(self, Self::Lower)
    }
}
