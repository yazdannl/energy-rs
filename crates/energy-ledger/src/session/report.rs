//! What one extraction window produced.

/// Sessions written, sessions stored with an `invalidated_reason`, and runs too short to be booked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExtractReport {
    pub written: usize,
    pub flagged: usize,
    pub skipped: usize,
}
