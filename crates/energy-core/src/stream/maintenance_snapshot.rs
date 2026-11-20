//! One pack's live maintenance phase.

/// The maintenance phase the engine assigned to one pack in the current plan.
#[derive(Debug, Clone, PartialEq)]
pub struct MaintenanceSnapshot {
    /// `charging` before the last cheap slot, `dwell` while balancing at 100 %.
    pub state: &'static str,
    /// The balancing target, 100 %.
    pub target_soc_pct: f64,
    /// End of the last planned charge slot, UTC epoch seconds.
    pub charge_end: i64,
    /// Earliest instant the dwell may complete, UTC epoch seconds.
    pub dwell_until: i64,
}
