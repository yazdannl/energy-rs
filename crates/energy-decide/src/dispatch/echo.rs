//! Device echo: a preliminary gate, never delivery evidence.

/// Echo of an accepted setpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Echo {
    /// Echoed limit in W, charge positive.
    pub limit_w: f64,
    /// Echo observation instant, UTC epoch milliseconds.
    pub at_ms: i64,
}

impl Echo {
    /// Whether the echo matches the issued command exactly.
    // Echo equality is a device limit comparison, not a numerical tolerance decision.
    #[allow(clippy::float_cmp)]
    pub fn matches(&self, command_w: f64) -> bool {
        self.limit_w == command_w
    }
}
