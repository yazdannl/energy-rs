//! Logged operator command required before the state machine may enter `clearing`.

/// Operator clearance request; all fields are mandatory and auditable.
#[derive(Debug, Clone)]
pub struct ClearanceCommand {
    /// Named operator.
    pub actor: String,
    /// Command instant, epoch milliseconds.
    pub at_ms: i64,
    /// Unique clearance id used in the per-pack barrier identity.
    pub clearance_id: String,
    /// True only after the health check required by M5 passed.
    pub health_ok: bool,
}

impl ClearanceCommand {
    /// Build a logged command.
    pub fn new(actor: &str, at_ms: i64, clearance_id: &str, health_ok: bool) -> Self {
        Self {
            actor: actor.to_owned(),
            at_ms,
            clearance_id: clearance_id.to_owned(),
            health_ok,
        }
    }

    /// True when actor, instant, clearance id and health check are all present.
    pub fn valid(&self) -> bool {
        !self.actor.is_empty() && self.at_ms > 0 && !self.clearance_id.is_empty() && self.health_ok
    }
}
