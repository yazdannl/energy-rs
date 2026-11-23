//! Requested versus actually covered slots; both survive every outcome.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coverage {
    /// Slots asked for, never shortened by a fallback.
    pub requested_slots: usize,
    /// Count of contiguous valid required slots from the containing slot.
    pub actual_slots: usize,
    /// Smallest accepted coverage (24 h).
    pub minimum_slots: usize,
    /// Exclusive end of the covered region, when at least one slot exists.
    pub coverage_end: Option<i64>,
    /// Number of missing tail slots, zero when full or when an interior gap refuses.
    pub missing_slots: usize,
    /// First absent or misaligned required slot start, when one exists.
    pub first_missing_t_start: Option<i64>,
}

impl Coverage {
    /// Whether the covered region is a 96..119-slot tail.
    pub const fn is_tail(self) -> bool {
        self.actual_slots >= self.minimum_slots && self.actual_slots < self.requested_slots
    }
}
