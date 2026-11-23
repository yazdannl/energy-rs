//! Why a configured source has no rows.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyReason {
    /// Every required source in the category is empty.
    AllEmpty,
    /// Some rows are present but at least one required slot is absent.
    MissingSlots,
}
