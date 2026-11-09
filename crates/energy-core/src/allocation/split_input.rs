//! The inputs of one fleet split, so the call site stays readable.

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
}

impl Default for SplitInput<'_> {
    fn default() -> Self {
        Self {
            reserve_soc_pct: 0.0,
            slot_hours: 0.25,
            block_discharge: &[],
            min_active_w: 50.0,
            staggering: true,
        }
    }
}
