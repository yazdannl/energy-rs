//! Confirmation and spacing for a power-triggered replan.

use crate::engine::Engine;
use crate::engine::error::EngineError;
use crate::replan::POWER_REPLAN_FLOOR_S;

/// How many consecutive ticks must cross the threshold before a solve is bought.
const CONFIRM_TICKS: u8 = 2;

impl Engine {
    /// Whether this tick may solve on a power move: the deviation has held, and the floor allows it.
    pub(crate) fn power_settled(&self, now: i64) -> Result<bool, EngineError> {
        let mut state = self.lock()?;
        state.power_streak = state.power_streak.saturating_add(1);
        if state.power_streak < CONFIRM_TICKS {
            return Ok(false);
        }
        let floor = POWER_REPLAN_FLOOR_S.max(self.config.replan_seconds / 4);
        let allowed = state
            .last_power_replan
            .is_none_or(|at| now.saturating_sub(at) >= floor);
        if allowed {
            state.last_power_replan = Some(now);
            state.power_streak = 0;
        }
        Ok(allowed)
    }

    /// The deviation went away: a later move starts its confirmation from scratch.
    pub(crate) fn reset_power_streak(&self) -> Result<(), EngineError> {
        self.lock()?.power_streak = 0;
        Ok(())
    }
}
