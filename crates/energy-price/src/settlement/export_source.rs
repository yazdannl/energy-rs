//! Which side of the meter an exported kWh came from.  The Zonnebonus is granted on PV export only
//! and never on home-battery export.

/// Provenance of an export (or import) leg for the bonus guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportSource {
    /// Solar production.
    Pv,
    /// The home battery discharging to the grid.
    Battery,
    /// Anything else (curtailment fallback, unknown measurement source).
    Other,
}

impl ExportSource {
    /// Whether this source counts as PV for the Zonnebonus.
    pub const fn is_pv(self) -> bool {
        matches!(self, ExportSource::Pv)
    }
}
