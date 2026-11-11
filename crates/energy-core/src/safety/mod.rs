//! The pure clamp ladder and its grid-follow correction.
//!
//! Every entry point here is synchronous and side-effect free: no clock reads, no I/O, no module
//! state. The dispatcher (P9) owns the loop and hands in `now`, the measurements and the plan.

mod authority;
mod check;
mod decision;
mod finish;
mod grid_follow;
mod ladder;
mod limits;
mod measured;
mod reason;
mod request;

pub use authority::Authority;
pub use check::check;
pub use decision::SafetyDecision;
pub use grid_follow::GridFollow;
pub use limits::SafetyLimits;
pub use measured::Measured;
pub use reason::Reason;
pub use request::CommandRequest;

/// A per-pack write step bound, W/tick (survey §7 trap 6).
pub const RAMP_LIMIT_W: f64 = 1000.0;
/// Default deadband for the grid-follow correction, W.
pub const GRID_FOLLOW_DEADBAND_W: f64 = 10.0;
/// Minimum grid-follow budget, W (survey §7 trap 11).
pub const GRID_FOLLOW_MIN_BUDGET_W: f64 = 500.0;
/// Values below this many watts are treated as zero.
pub(crate) const EPS_W: f64 = 0.5;
