//! One pack's requested setpoint in one slot; the plan and M5 own its meaning.

/// Request handed to dispatch; M4 never changes the requested power.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotRequest {
    /// Stable pack identifier.
    pub pack_id: String,
    /// Requested battery power in W, charge positive.
    pub requested_w: f64,
    /// Slot instant, UTC epoch milliseconds.
    pub at_ms: i64,
    /// True only when M5 has already classified this as safety traffic.
    pub safety_traffic: bool,
    /// Reason supplied by the rule that produced the request.
    pub reason: &'static str,
}

impl SlotRequest {
    /// Build an ordinary economic request.
    pub fn economic(pack_id: &str, requested_w: f64, at_ms: i64, reason: &'static str) -> Self {
        Self {
            pack_id: pack_id.to_owned(),
            requested_w,
            at_ms,
            safety_traffic: false,
            reason,
        }
    }

    /// Build a safety-lane request without deciding safety in M4.
    pub fn safety(pack_id: &str, requested_w: f64, at_ms: i64, reason: &'static str) -> Self {
        Self {
            pack_id: pack_id.to_owned(),
            requested_w,
            at_ms,
            safety_traffic: true,
            reason,
        }
    }
}
