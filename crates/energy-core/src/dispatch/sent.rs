//! The result of one transport attempt, before it becomes a `PackReport`.

/// What one send attempt produced.
#[derive(Debug)]
pub(crate) struct Sent {
    pub(crate) written: bool,
    pub(crate) verified: bool,
    pub(crate) applied_w: f64,
    pub(crate) fail_safe: bool,
    /// Why a safety hold took over this attempt; ownership stays with that hold.
    pub(crate) superseded: Option<String>,
    pub(crate) error: Option<String>,
}

impl Sent {
    /// Nothing reached a device: no write, no receipt, no fault.
    pub(crate) const fn none() -> Self {
        Self {
            written: false,
            verified: false,
            applied_w: 0.0,
            fail_safe: false,
            superseded: None,
            error: None,
        }
    }
}
