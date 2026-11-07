//! What the engine publishes for one instant.

use crate::safety::Authority;
use crate::stream::{
    FaultSnapshot, ForecastSnapshot, MeterSnapshot, PackSnapshot, PlanSnapshot, SiteSnapshot,
};

/// The engine's read model at one instant.
///
/// Everything the HTTP API answers from a read request is in here (the ledger supplies history:
/// stored plans, dispatch rows, savings days). The snapshot is built by the engine after each
/// decision, so a reader sees a consistent set of fields instead of a torn mix.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EngineSnapshot {
    /// The mode the engine is currently obeying.
    pub mode: Authority,
    /// The discovered site: sources and the capabilities the engine actually has.
    pub site: SiteSnapshot,
    /// The plan the engine is following, when there is one.
    pub plan: Option<PlanSnapshot>,
    /// Per-pack state, learned profile, latch and manual override.
    pub packs: Vec<PackSnapshot>,
    /// Live grid/PV/load power, when HA published a fresh reading.
    pub meter: Option<MeterSnapshot>,
    /// The forecast totals this slot's plan was built on.
    pub forecast: Option<ForecastSnapshot>,
    /// When the engine last completed a tick, UTC epoch seconds.
    pub last_tick: Option<i64>,
    /// Open faults, newest last. A fault is cleared by the engine, not by the reader.
    pub faults: Vec<FaultSnapshot>,
}
