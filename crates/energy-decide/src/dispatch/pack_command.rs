//! Device command emitted by dispatch after both consumer ports allow it.

use crate::dispatch::SlotRequest;

/// A command whose transport attempt is bounded by M4's per-pack budget.
#[derive(Debug, Clone, PartialEq)]
pub struct PackCommand {
    /// Stable pack identifier.
    pub pack_id: String,
    /// Requested battery power in W, charge positive.
    pub requested_w: f64,
    /// Issue instant, UTC epoch milliseconds.
    pub issued_at_ms: i64,
    /// Monotonic generation assigned when the write was prepared.
    pub generation: u64,
    /// Fence supplied by M5 for safety traffic, when one was given.
    pub fence_id: Option<String>,
    /// True only for M5-classified safety traffic.
    pub safety_traffic: bool,
}

impl PackCommand {
    /// Convert a request into a command; no authority decision happens here.
    pub fn from_request(request: &SlotRequest) -> Self {
        Self {
            pack_id: request.pack_id.clone(),
            requested_w: request.requested_w,
            issued_at_ms: request.at_ms,
            generation: 0,
            fence_id: None,
            safety_traffic: request.safety_traffic,
        }
    }
}
