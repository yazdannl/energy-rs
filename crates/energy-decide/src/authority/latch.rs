//! Independent durable latch row that takes precedence over persisted operational mode.

use crate::authority::{AuthorityState, LatchToken};

/// The durable authority row read before every settings/ledger write.
#[derive(Debug, Clone)]
pub struct Latch {
    token: LatchToken,
    state: AuthorityState,
    reason: String,
    created_at_ms: i64,
    last_transition_ms: i64,
}

impl Latch {
    /// Create the initial latch row with an explicit, auditable reason.
    pub fn new(token: LatchToken, state: AuthorityState, reason: &str, at_ms: i64) -> Self {
        Self {
            token,
            state,
            reason: reason.to_owned(),
            created_at_ms: at_ms,
            last_transition_ms: at_ms,
        }
    }

    /// Authority token.
    pub fn token(&self) -> &LatchToken {
        &self.token
    }
    /// Current operational authority state.
    pub fn state(&self) -> AuthorityState {
        self.state
    }
    /// Explicit reason for this state; never silently empty.
    pub fn reason(&self) -> &str {
        &self.reason
    }
    /// Instant the row was created.
    pub fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }
    /// Instant of the last state transition.
    pub fn last_transition_ms(&self) -> i64 {
        self.last_transition_ms
    }

    /// Apply a logged transition; the caller must already have passed the gate.
    pub(crate) fn transition(&mut self, state: AuthorityState, reason: &str, at_ms: i64) {
        self.state = state;
        self.reason = String::from(reason);
        self.last_transition_ms = at_ms;
    }

    /// Storage seam: simulate a manual edit of the persisted generation text.
    pub fn corrupt_generation(&mut self, raw: &str) {
        let seed = self.token.hash().to_owned();
        self.token = LatchToken::from_storage(raw, &seed);
    }
}
