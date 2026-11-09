//! One latched pack selection.

/// Which pack the fleet picked, for which direction, since when.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionRow {
    /// `"charge"` or `"discharge"`.
    pub direction: String,
    /// The pack that holds the slot.
    pub holder: String,
    /// When the hold started, UTC epoch seconds.
    pub selected_at: i64,
    /// The fleet target the hold was taken for, W.
    pub target_w: f64,
}
