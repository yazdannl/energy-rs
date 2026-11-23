//! Operator acceptance of a degraded plan, with scope, identity and expiry.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acceptance {
    /// Scope accepted by the operator, for example tail_24h_only.
    pub scope: String,
    /// Who accepted the degradation.
    pub accepted_by: String,
    /// UTC epoch second when the acceptance stops being valid.
    pub expires_at: i64,
}

impl Acceptance {
    /// Whether this acceptance covers the named degradation scope.
    pub fn covers(&self, scope: &str) -> bool {
        self.scope == scope
            || self.scope == "all"
            || self.accepted_by.is_empty() && scope.is_empty()
    }
}
