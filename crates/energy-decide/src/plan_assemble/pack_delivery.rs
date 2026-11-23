//! Typed per-pack delivery evidence.

#[derive(Debug, Clone, PartialEq)]
pub struct PackDelivery {
    /// Pack identifier.
    pub pack_id: String,
    /// Requested power, W.
    pub planned_w: f64,
    /// Freshly measured power, W.
    pub applied_w: f64,
    /// Typed reason slug, for example orphan_hold or measured.
    pub reason: &'static str,
    /// Written status recorded by the caller, for example attempted.
    pub written: &'static str,
}
