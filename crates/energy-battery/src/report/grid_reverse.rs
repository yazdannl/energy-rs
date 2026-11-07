//! `gridReverse`: whether the inverter may push PV to the grid.

/// Export permission reported by the device.
///
/// The vendor documentation gives no labels; the values come from the two independent
/// implementations: 0 disabled, 1 export allowed, 2 export forbidden.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GridReverse {
    #[default]
    Disabled,
    AllowExport,
    ForbidExport,
    Other(i32),
}

impl GridReverse {
    pub const fn from_raw(raw: i32) -> Self {
        match raw {
            0 => Self::Disabled,
            1 => Self::AllowExport,
            2 => Self::ForbidExport,
            other => Self::Other(other),
        }
    }

    /// `Some(true)` when export is allowed, `Some(false)` when it is forbidden, `None` when the
    /// device reports a value nobody has documented.
    pub const fn allows_export(self) -> Option<bool> {
        match self {
            Self::AllowExport => Some(true),
            Self::ForbidExport => Some(false),
            Self::Disabled | Self::Other(_) => None,
        }
    }
}
