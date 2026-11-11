//! How long a refused solve waits before the next attempt.

use crate::engine::Engine;
use crate::engine::error::EngineError;
use crate::replan::REFUSAL_BACKOFF_S;

impl Engine {
    /// Whether the last refusal is still inside its backoff window.
    ///
    /// The tick keeps holding while it is: a refusal means the *inputs* are unusable, and the answer
    /// to unusable inputs is to wait, not to ask again a second later.
    pub(crate) fn refusal_holds_off(&self, now: i64) -> Result<bool, EngineError> {
        let state = self.lock()?;
        let Some(refusal) = state.refusal.as_ref() else {
            return Ok(false);
        };
        let backoff = REFUSAL_BACKOFF_S.max(self.config.replan_seconds / 10);
        Ok(now.saturating_sub(refusal.at) < backoff)
    }
}
