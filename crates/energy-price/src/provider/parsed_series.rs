//! Parsed entries plus the resolution the payload advertises.

use super::RawSlot;

/// Parsed entries plus the resolution the payload advertises.
#[derive(Debug, Clone)]
pub struct ParsedSeries {
    /// Entries in payload order (not yet sorted, filtered or validated).
    pub raw: Vec<RawSlot>,
    /// Advertised slot duration in seconds (900 or 3600 for electricity).
    pub step: i64,
}
