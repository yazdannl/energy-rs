//! A latched pack selection, as the split reads it.

/// The pack that holds the current cycle, and why it keeps the slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hold<'a> {
    /// The latched pack.
    pub holder: &'a str,
    /// `"charge"` or `"discharge"`: a hold never survives a sign flip.
    pub direction: &'a str,
    /// When the hold started, UTC epoch seconds.
    pub since: i64,
}
