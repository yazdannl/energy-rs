//! The authority gate: only `auto` may write.

/// What the operator allowed this process to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// Compute and log, but never write.
    Advisory,
    /// Run the full pipeline and journal the intended write, but never write.
    DryRun,
    /// May write.
    Auto,
}

impl Authority {
    /// Whether anything at all may reach a device, including an exit hold.
    pub const fn may_write(self) -> bool {
        matches!(self, Self::Auto)
    }

    /// Stable slug for logs and settings.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Advisory => "advisory",
            Self::DryRun => "dry_run",
            Self::Auto => "auto",
        }
    }
}
