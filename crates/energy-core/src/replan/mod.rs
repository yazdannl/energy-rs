//! Replan triggers: why the current plan must be replaced, or nothing.

mod changes;
mod live_state;
mod power_move;
mod reason;
mod trigger;

pub use changes::Changes;
pub use live_state::LiveState;
pub use power_move::PowerMove;
pub use reason::ReplanReason;
pub use trigger::ReplanTrigger;

/// SoC deviation that invalidates a plan, percent (survey §7 trap 17).
pub const SOC_DEVIATION_PCT: f64 = 3.0;
/// Natural plan resolution, seconds.
pub const REPLAN_SECONDS: i64 = 900;
/// Default absolute stale bound, seconds.
pub const STALE_PLAN_SECONDS: i64 = 1800;
/// Default grid deviation that forces a re-solve, W.
pub const GRID_REACT_W: f64 = 10.0;
/// Default PV deviation that forces a re-solve, W.
pub const SOLAR_REACT_W: f64 = 10.0;
/// Floor between two power-triggered solves, seconds.
pub const POWER_REPLAN_FLOOR_S: i64 = 10;
