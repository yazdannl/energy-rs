//! The inputs of one fleet split, so the call site stays readable.

use crate::allocation::Hold;

/// Everything the split needs besides the target: pack bounds, the tick length and the policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitInput<'a> {
    /// SoC floor kept for the operator's reserve, percent.
    pub reserve_soc_pct: f64,
    /// Tick length, hours.
    pub slot_hours: f64,
    /// Packs the protection layer will not discharge.
    pub block_discharge: &'a [&'a str],
    /// Smallest allocation worth starting a pack for, W.
    pub min_active_w: f64,
    /// Run as few packs as the request needs (off = proportional spread).
    pub staggering: bool,
    /// The latched selection, if any.
    pub hold: Option<Hold<'a>>,
    /// SoC differences below this count as a tie, percent.
    pub soc_band_pct: f64,
    /// Now, UTC epoch seconds.
    pub now: i64,
    /// How long a hold keeps the slot, seconds.
    pub hold_seconds: i64,
}

impl Default for SplitInput<'_> {
    fn default() -> Self {
        Self {
            reserve_soc_pct: 0.0,
            slot_hours: 0.25,
            block_discharge: &[],
            min_active_w: 50.0,
            staggering: true,
            hold: None,
            soc_band_pct: 2.0,
            now: 0,
            hold_seconds: 1800,
        }
    }
}
