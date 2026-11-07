//! Who asked for an override: the operator or the maintenance window.

/// Where an override came from, so the journal can tell a person from a schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideSource {
    /// An operator command from the UI/API.
    Manual,
    /// The weekly full-charge maintenance window.
    Maintenance,
}

impl OverrideSource {
    /// A stable slug for logs.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Maintenance => "maintenance",
        }
    }
}
