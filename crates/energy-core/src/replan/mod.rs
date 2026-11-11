//! Replan triggers: why the current plan must be replaced, or nothing.

mod changes;
mod live_state;
mod reason;
mod trigger;

pub use changes::Changes;
pub use live_state::LiveState;
pub use reason::ReplanReason;
pub use trigger::ReplanTrigger;

/// SoC deviation that invalidates a plan, percent (survey §7 trap 17).
pub const SOC_DEVIATION_PCT: f64 = 3.0;
/// Natural replan interval, seconds: the plan is refreshed, the meter is followed every tick.
pub const REPLAN_SECONDS: i64 = 300;
/// Default absolute stale bound, seconds.
pub const STALE_PLAN_SECONDS: i64 = 1800;
