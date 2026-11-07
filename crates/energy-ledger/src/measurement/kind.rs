//! What produced a sample. Stored with the row so a replayed or derived sample can be told apart
//! from a device report.

/// Source of a 1-minute sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleKind {
    /// A report read from the pack over zenSDK.
    Report,
    /// A derived/replayed sample (shadow runs, backfills).
    Derived,
}

impl SampleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Report => "report",
            Self::Derived => "derived",
        }
    }
}
