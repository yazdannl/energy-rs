//! One planned versus delivered power comparison row.

#[derive(Debug, Clone, PartialEq)]
pub struct PlanVsDelivered {
    /// Slot start, UTC epoch seconds.
    pub slot: i64,
    /// Planned pack power, W.
    pub planned_w: f64,
    /// Measured pack power, W.
    pub delivered_w: f64,
    /// Planned minus delivered, W.
    pub delta_w: f64,
}
