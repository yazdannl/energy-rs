//! What one grid-follow tick is asked to do.

/// What one tick asks the fleet for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FollowRequest {
    /// The plan's own setpoint, W (positive = charging).
    pub plan_w: f64,
    /// The battery's measured AC output, W (positive = charging).
    pub battery_w: f64,
    /// The grid the site wants to see, W (positive = importing).
    pub desired_grid_w: f64,
    /// The grid the meter sees now, W.
    pub measured_grid_w: f64,
    /// The fleet's AC capability, W.
    pub capability_w: f64,
}
