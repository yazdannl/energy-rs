//! Delivery is independent of plan feasibility: one pack can fail without failing the fleet.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// Every requested pack delivered.
    Full,
    /// At least one pack delivered, at least one failed.
    Partial,
    /// No requested pack delivered.
    Failed,
}
