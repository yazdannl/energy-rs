//! What one tick did, per pack.

use crate::dispatch::PackReport;
use crate::safety::{Authority, Reason};

/// The dispatcher's answer for one tick.
#[derive(Debug, Clone, PartialEq)]
pub struct TickOutcome {
    /// Tick instant, UTC epoch seconds.
    pub at: i64,
    /// The authority the tick ran under.
    pub mode: Authority,
    /// Whether the plan was refused as stale (every pack held).
    pub stale_input: bool,
    /// The interpolated plan setpoint, W; `None` when the plan could not be executed.
    pub plan_power_w: Option<f64>,
    /// The fleet target after the grid clamp, W.
    pub target_w: f64,
    /// The fleet-level grid bound, when it bit.
    pub grid_limit: Option<Reason>,
    /// The final-vector bound (fleet power cap or site ceiling) when it had to reduce the commands.
    pub fleet_clamp: Option<Reason>,
    /// One row per pack, in fleet order.
    pub packs: Vec<PackReport>,
}

impl TickOutcome {
    /// An empty outcome for one tick.
    pub fn new(at: i64, mode: Authority) -> Self {
        Self {
            at,
            mode,
            stale_input: false,
            plan_power_w: None,
            target_w: 0.0,
            grid_limit: None,
            fleet_clamp: None,
            packs: Vec::new(),
        }
    }

    /// How many commands actually reached a device.
    pub fn writes(&self) -> usize {
        self.packs.iter().filter(|pack| pack.written).count()
    }

    /// One pack's row.
    pub fn report(&self, pack_id: &str) -> Option<&PackReport> {
        self.packs.iter().find(|pack| pack.pack_id == pack_id)
    }

    /// Whether every pack was held at zero (or skipped) without a write.
    pub fn is_quiet(&self) -> bool {
        self.packs.iter().all(|pack| !pack.written)
    }
}
