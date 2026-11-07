//! The authority gate: only `auto` may write.

use serde::{Deserialize, Serialize};

/// What the operator allowed this process to do.
///
/// The three slugs are shared by the ledger setting (`control.mode`), the logs and the HTTP
/// contract, so the enum carries the serde spelling itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    /// Compute and log, but never write.
    Advisory,
    /// Run the full pipeline and journal the intended write, but never write.
    #[default]
    DryRun,
    /// May write.
    Auto,
}

impl Authority {
    /// Whether anything at all may reach a device, including an exit hold.
    pub const fn may_write(self) -> bool {
        matches!(self, Self::Auto)
    }

    /// Stable slug for logs, settings and the API.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Advisory => "advisory",
            Self::DryRun => "dry_run",
            Self::Auto => "auto",
        }
    }

    /// Strict parse of an API/stored slug; an unknown value is `None` (the caller decides whether
    /// that is the safe default or a validation error).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "advisory" => Some(Self::Advisory),
            "dry_run" => Some(Self::DryRun),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    /// Every mode, in escalation order.
    pub const fn all() -> [Self; 3] {
        [Self::Advisory, Self::DryRun, Self::Auto]
    }
}
