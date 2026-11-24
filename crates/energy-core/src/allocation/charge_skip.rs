//! The typed, operator-visible reason a pack was not offered charge this tick.

use crate::safety::Reason;

/// One pack deliberately excluded from a charge split.
///
/// A skip is a device capability verdict, not a transport failure: it must never be logged as a
/// read failure or a timeout, and it must never arm the fault backoff. `note` carries the device
/// evidence (reported electricLevel, `socLimit`, `faultLevel`) so the dispatch row can say exactly
/// which protection closed the pack.
#[derive(Debug, Clone, PartialEq)]
pub struct ChargeSkip {
    /// The pack this verdict belongs to.
    pub pack_id: String,
    /// `charge_at_ceiling` for device protection, `stale_state` for an untrustworthy sample.
    pub reason: Reason,
    /// Human-readable device evidence for the dispatch log and API.
    pub note: String,
}

impl ChargeSkip {
    /// A named skip for one pack.
    pub fn new(pack_id: impl Into<String>, reason: Reason, note: impl Into<String>) -> Self {
        Self {
            pack_id: pack_id.into(),
            reason,
            note: note.into(),
        }
    }
}
