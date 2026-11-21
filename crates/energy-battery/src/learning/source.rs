//! Where a published capacity came from, so an operator can trust (or ignore) it.

use serde::{Deserialize, Serialize};

/// The evidence behind `capacity_kwh`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacitySource {
    /// No useful evidence yet: the label (or the pack's own report) is standing in.
    #[default]
    Label,
    /// The identical-label peer pool supplied the value; this pack had little evidence of its own.
    Pooled,
    /// This pack's own long-span cycles moved the number away from the prior.
    Own,
    /// A thin pass carried forward the previous profile.
    Previous,
}

impl CapacitySource {
    /// A stable slug for logs and the API.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Label => "label",
            Self::Pooled => "pooled",
            Self::Own => "own",
            Self::Previous => "previous",
        }
    }
}
