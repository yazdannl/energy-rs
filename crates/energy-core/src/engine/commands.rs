//! API commands, applied between ticks.
//!
//! The server validates a request and queues one [`EngineCommand`]; the loop drains that queue
//! before the next tick so a mode change or a manual override takes effect without waiting for a
//! plan. Every command is a request: the effect is visible in the next snapshot.

use energy_ledger::EventLevel;

use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::stream::EngineCommand;
use crate::wiring::sessions::SessionPolicy;

impl Engine {
    /// Apply one queued command. A failure is journalled by the caller; the loop keeps running.
    pub async fn apply_command(&self, command: EngineCommand, now: i64) -> Result<(), EngineError> {
        match command {
            EngineCommand::SetMode(mode) => self.set_mode(mode, now).await?,
            EngineCommand::Override(request) => {
                self.override_pack(request.pack, request.action, i64::from(request.ttl_s), now)?;
            }
            EngineCommand::FullCharge { pack } => self.request_full_charge(pack, now)?,
            EngineCommand::Relearn { pack } => self.relearn(pack.as_deref(), now)?,
            EngineCommand::Replan => self.replan_now(now).await?,
        }
        self.publish_meta(now);
        Ok(())
    }

    /// Solve a fresh plan now, as the operator asked, and journal the outcome.
    ///
    /// The plan is not dispatched from here: the next tick picks it up like any other, so a manual
    /// replan cannot bypass the trigger's freshness checks.
    async fn replan_now(&self, now: i64) -> Result<(), EngineError> {
        let view = self.fleet.view(now).await?;
        let solved = self.replan(now, &view).await.is_some();
        self.event(
            now,
            if solved {
                EventLevel::Info
            } else {
                EventLevel::Warn
            },
            "plan.requested",
            if solved {
                "operator asked for a replan; a fresh plan was solved".to_owned()
            } else {
                "operator asked for a replan; the solve was refused and the old plan is held"
                    .to_owned()
            },
        )?;
        Ok(())
    }

    /// Start the maintenance run now, for one pack or the whole fleet.
    fn request_full_charge(&self, pack: Option<String>, now: i64) -> Result<(), EngineError> {
        let mut state = self.lock()?;
        state.maintenance_pack = pack;
        state.maintenance.request_manual();
        let target = state
            .maintenance_pack
            .clone()
            .unwrap_or_else(|| "the whole fleet".to_owned());
        drop(state);
        self.event(
            now,
            EventLevel::Info,
            "maintenance.requested",
            format!("full charge requested for {target}"),
        )?;
        Ok(())
    }

    /// Relearn one pack or the fleet from the ledger's sessions, and store the result.
    fn relearn(&self, pack: Option<&str>, now: i64) -> Result<(), EngineError> {
        let policy = SessionPolicy::new(
            self.config.session_check_s,
            self.config.session_window_hours,
        );
        let reports = crate::wiring::sessions::relearn_now(
            &self.ledger,
            &self.learning_store,
            now,
            &policy,
            pack,
        )?;
        for report in &reports {
            self.event(now, EventLevel::Info, "learning.applied", report.summary())?;
        }
        self.event(
            now,
            EventLevel::Info,
            "learning.relearned",
            format!(
                "{} pack(s) relearned for {}",
                reports.len(),
                pack.unwrap_or("the whole fleet")
            ),
        )?;
        Ok(())
    }
}
