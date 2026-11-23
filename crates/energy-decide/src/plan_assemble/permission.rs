//! What dispatch may do with a plan; safety is never disabled by module M3.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchPermission {
    /// Economic and safety dispatch are both allowed.
    EconomicAndSafety,
    /// Economic dispatch only inside the accepted scope; safety is always active.
    ScopedEconomicAndSafety,
    /// No economic setpoint from this plan; safety lane only.
    SafetyOnly,
}
