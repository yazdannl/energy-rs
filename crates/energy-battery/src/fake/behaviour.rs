//! What the fake device does wrong on purpose.

use std::time::Duration;

/// Fault injection knobs.
///
/// The defaults are the measured device: a write that follows another within 1.3 s is acknowledged
/// but dropped. Everything else is opt-in so a test states the failure it wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeBehaviour {
    /// Writes closer together than this are acknowledged without being applied.
    pub drop_writes_below: Duration,
    /// Acknowledge writes but never apply them (the "lost write" case).
    pub ignore_writes: bool,
    /// Report `acMode` 127 and ignore limit writes (the wedged AC path).
    pub wedged: bool,
    /// Answer every request with this HTTP status (401 = local API disabled).
    pub http_status: Option<u16>,
    /// Answer every write with this body code (400 / 401 / 413).
    pub write_code: Option<i32>,
}

impl Default for FakeBehaviour {
    fn default() -> Self {
        Self {
            drop_writes_below: Duration::from_millis(1300),
            ignore_writes: false,
            wedged: false,
            http_status: None,
            write_code: None,
        }
    }
}

impl FakeBehaviour {
    /// Accept every write regardless of spacing (fast end-to-end tests).
    #[must_use]
    pub fn without_spacing_rule(mut self) -> Self {
        self.drop_writes_below = Duration::ZERO;
        self
    }

    /// Acknowledge but never apply writes.
    #[must_use]
    pub fn ignoring_writes(mut self) -> Self {
        self.ignore_writes = true;
        self
    }

    /// Report a wedged AC path.
    #[must_use]
    pub fn wedged(mut self) -> Self {
        self.wedged = true;
        self
    }

    /// Answer every request with `status`.
    #[must_use]
    pub fn with_http_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    /// Answer every write with `code`.
    #[must_use]
    pub fn with_write_code(mut self, code: i32) -> Self {
        self.write_code = Some(code);
        self
    }
}
