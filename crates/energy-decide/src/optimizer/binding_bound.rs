//! One binding bound: the structured evidence attached to a refusal or a clip.

/// A named hard bound that stopped a candidate action or an outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct BindingBound {
    /// Stable reason slug: `import_limit`, `export_limit`, `soc_floor`, `soc_ceiling`,
    /// `pack_power`, `min_pack_power` or `terminal_target`.
    pub reason: &'static str,
    /// Zero-based slot index; `N` for a terminal-only bound.
    pub slot: usize,
    /// The bound's value in kW or kWh.
    pub kwh_or_kw: f64,
}

impl BindingBound {
    /// Construct one binding bound.
    pub const fn new(reason: &'static str, slot: usize, kwh_or_kw: f64) -> Self {
        Self {
            reason,
            slot,
            kwh_or_kw,
        }
    }
}
