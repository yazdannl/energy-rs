//! What the ladder decided, and every bound that touched it.

use crate::safety::Reason;

/// The approved setpoint and its reasons.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyDecision {
    /// What the dispatcher asked for, W.
    pub requested_w: f64,
    /// What may be written, W (0 when blocked).
    pub allowed_w: f64,
    /// Whether every bound could not be satisfied together.
    pub blocked: bool,
    /// Whether the approved value differs from the request.
    pub clamped: bool,
    /// Lowest feasible power over the applied bounds, W.
    pub lo_w: f64,
    /// Highest feasible power over the applied bounds, W.
    pub hi_w: f64,
    /// Every bound that moved the value, or every binding bound on a block.
    pub reasons: Vec<Reason>,
}

impl SafetyDecision {
    pub(crate) fn blocked(requested_w: f64, lo_w: f64, hi_w: f64, reasons: Vec<Reason>) -> Self {
        Self {
            requested_w,
            allowed_w: 0.0,
            blocked: true,
            clamped: false,
            lo_w,
            hi_w,
            reasons,
        }
    }

    pub(crate) fn allowed(
        requested_w: f64,
        allowed_w: f64,
        lo_w: f64,
        hi_w: f64,
        reasons: Vec<Reason>,
    ) -> Self {
        let reasons = if reasons.is_empty() {
            vec![Reason::Ok]
        } else {
            reasons
        };
        Self {
            requested_w,
            allowed_w,
            blocked: false,
            clamped: (allowed_w - requested_w).abs() > super::EPS_W,
            lo_w,
            hi_w,
            reasons,
        }
    }

    /// Whether a write should reach the device.
    pub fn writes(&self) -> bool {
        !self.blocked && self.allowed_w.abs() > super::EPS_W
    }

    /// Whether a reason is present.
    pub fn has(&self, reason: Reason) -> bool {
        self.reasons.contains(&reason)
    }
}
