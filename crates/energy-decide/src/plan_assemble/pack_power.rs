//! One pack's planned or measured power, charge positive.

#[derive(Debug, Clone, PartialEq)]
pub struct PackPower {
    /// Stable pack identifier.
    pub pack_id: String,
    /// Power in W; positive is charge, negative is discharge.
    pub w: f64,
}
