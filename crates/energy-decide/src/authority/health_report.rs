//! Health payload shared verbatim by the dashboard and the API.

/// Visible protection status; text is identical in dashboard and API health.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthReport {
    /// Typed protection/battery codes joined by commas in the API payload.
    pub codes: Vec<&'static str>,
    /// Exact prominent human-readable text.
    pub text: &'static str,
    /// Universal startup banner; continuity is never claimed.
    pub banner: &'static str,
    /// True only in auto including protection_active.
    pub safety_active: bool,
    /// True in latched, clearing and recovery.
    pub battery_unmanaged: bool,
}

impl HealthReport {
    /// API health payload string, preserving the frozen code order.
    pub fn code_string(&self) -> String {
        self.codes.join(",")
    }
}
