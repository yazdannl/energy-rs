//! The guard step: the watchdog hold, release, shutdown and the hold primitive.

use energy_ledger::{DispatchEntry, EventLevel};

use crate::dispatch::{CommandSink, SinkError};
use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::safety::Reason;

impl Engine {
    /// One guard step: hold everything if the loop stopped making progress, then retry boot recovery.
    ///
    /// The generation is fenced *before* the hold, so a tick that is still in flight can no longer
    /// write the setpoint this hold supersedes.
    pub async fn supervise(&self, now: i64) -> Result<bool, EngineError> {
        let stalled = self.lock()?.watchdog.stalled(now);
        let mode = self.gate.mode();
        let mut held = false;
        if stalled {
            self.lock()?.watchdog.mark(now);
            if mode.may_write() {
                self.gate.fence();
                self.hold_all(now, Reason::WatchdogHold, None).await?;
                held = true;
            }
            self.event(
                now,
                EventLevel::Error,
                "watchdog.hold",
                format!(
                    "no control progress for {} s; {}",
                    self.config.stall_after_s(),
                    if held {
                        "held all packs".to_owned()
                    } else {
                        format!("nothing written in {} mode", mode.slug())
                    }
                ),
            )?;
        }
        self.retry_boot_latches(now).await?;
        Ok(held)
    }

    /// Neutralise what we left running because control is ending.
    ///
    /// In `auto` every pack is held. Outside `auto` nothing at all is written -- not even a hold --
    /// so the latch is kept and boot recovery owns those packs from then on.
    pub async fn shutdown(&self, now: i64, reason: &str) -> Result<(), EngineError> {
        let mode = self.gate.mode();
        let pending = self.lock()?.latch.pack_ids().len();
        self.gate.fence();
        let held = if mode.may_write() {
            self.hold_all(now, Reason::Shutdown, None).await?
        } else {
            Vec::new()
        };
        let detail = if mode.may_write() {
            format!("{} pack(s) held", held.len())
        } else {
            format!(
                "nothing written in {} mode, {pending} latched pack(s) kept",
                mode.slug()
            )
        };
        self.event(
            now,
            EventLevel::Info,
            "engine.shutdown",
            format!("{reason}: {detail}"),
        )?;
        Ok(())
    }

    /// Neutralise the latched packs before leaving `auto` (mode change, dry-run flip).
    ///
    /// Called while the engine is still in `auto`, so the holds are admitted; a hold that fails keeps
    /// its latch entry and does not block the mode change -- stopping the writes matters more.
    pub(crate) async fn release(&self, now: i64) -> Result<(), EngineError> {
        let pending = self.lock()?.latch.pack_ids();
        if pending.is_empty() {
            return Ok(());
        }
        if let Err(error) = self.hold_all(now, Reason::Shutdown, Some(&pending)).await {
            self.event(
                now,
                EventLevel::Error,
                "hold.failed",
                format!("release: {error}"),
            )?;
        }
        Ok(())
    }

    /// Hold every pack (or just `only`), recording each attempt and clearing verified holds.
    ///
    /// Every target is attempted even when bookkeeping fails for one of them; the failures are
    /// returned together, and the packs they concern keep their latch entry.
    pub(crate) async fn hold_all(
        &self,
        now: i64,
        reason: Reason,
        only: Option<&[String]>,
    ) -> Result<Vec<String>, EngineError> {
        let ids = self.hold_targets(only)?;
        let mut held = Vec::new();
        let mut failures = Vec::new();
        for (pack_id, ledger_id) in ids {
            match self.hold_one(&pack_id, ledger_id, now, reason).await {
                Ok(true) => held.push(pack_id),
                Ok(false) => {}
                Err(error) => failures.push(format!("{pack_id}: {error}")),
            }
        }
        if failures.is_empty() {
            return Ok(held);
        }
        Err(EngineError::Holds {
            failed: failures.len(),
            detail: failures.join("; "),
        })
    }

    /// Who a hold targets: an explicit list, or every known pack plus anything still latched.
    ///
    /// Deliberately no fleet read: the watchdog fires exactly when the loop is stuck, so a hold that
    /// needs the stuck read would never happen. A latched pack the fleet no longer reports is still
    /// held, because it is hardware we drove.
    fn hold_targets(
        &self,
        only: Option<&[String]>,
    ) -> Result<Vec<(String, Option<i64>)>, EngineError> {
        let Some(ids) = only else {
            let state = self.lock()?;
            let mut targets = state.known_packs.clone();
            for pack_id in state.latch.pack_ids() {
                if !targets.iter().any(|(known, _)| *known == pack_id) {
                    targets.push((pack_id, None));
                }
            }
            return Ok(targets);
        };
        Ok(ids.iter().map(|pack_id| (pack_id.clone(), None)).collect())
    }

    /// One real hold through the gate; a verified echo clears the pack's latch.
    async fn hold_one(
        &self,
        pack_id: &str,
        ledger_id: Option<i64>,
        now: i64,
        reason: Reason,
    ) -> Result<bool, EngineError> {
        let token = self.gate.token();
        match self.guarded(token, now).hold(pack_id).await {
            Ok(receipt) => {
                if receipt.verified {
                    self.clear_owned(pack_id, now)?;
                }
                let entry = DispatchEntry::new(
                    now,
                    ledger_id,
                    0.0,
                    receipt.applied_w,
                    reason.as_str(),
                    "auto",
                );
                let entry = if receipt.verified {
                    entry.verified(receipt.applied_w)
                } else {
                    entry.failed("hold not echoed")
                };
                self.ledger.dispatch()?.insert(&entry)?;
                Ok(true)
            }
            // Authority or generation withdrew the command: nothing was sent and ownership stays.
            Err(SinkError::Fenced { .. }) => Ok(false),
            Err(error) => {
                self.event(
                    now,
                    EventLevel::Error,
                    "hold.failed",
                    format!("{pack_id}: {error}"),
                )?;
                Ok(false)
            }
        }
    }

    /// Drop a pack's ownership durably, and only after a verified hold.
    ///
    /// The new latch is persisted first and committed in memory only when the write succeeded, so a
    /// ledger failure can never lose track of a pack that was actually driven.
    fn clear_owned(&self, pack_id: &str, now: i64) -> Result<(), EngineError> {
        let mut state = self.lock()?;
        if state.latch.get(pack_id).is_none() {
            return Ok(());
        }
        let mut next = state.latch.clone();
        next.clear(pack_id);
        next.save(&self.ledger, now)?;
        state.latch = next;
        Ok(())
    }
}
