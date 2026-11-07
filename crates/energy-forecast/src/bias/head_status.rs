//! What the promotion gate decided.

/// What the gate decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadStatus {
    /// Not enough scored slots or hours.
    Insufficient,
    /// The candidate did not beat the incumbent by the margin.
    Rejected,
    /// The candidate beat the incumbent and replaces it.
    Promoted,
}

impl HeadStatus {
    /// The status string stored in the payload.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insufficient => "insufficient",
            Self::Rejected => "rejected",
            Self::Promoted => "promoted",
        }
    }
}
