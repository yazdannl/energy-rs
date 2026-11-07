//! Boot neutralisation, the watchdog hold, release and shutdown.

use energy_ledger::{DispatchEntry, EventLevel};

use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::engine::live_latch::LiveLatch;
use crate::safety::Reason;

impl Engine {
    /// Load the persisted mode and latch, restore the protection state and neutralise boot setpoints.
    ///
    /// Only packs in the live-write latch are touched: a fresh install has no latch, so boot writes
    /// nothing at all (survey §7 trap 2).
    pub async fn boot(&self, now: i64) -> Result<(), EngineError> {
        let mode = self.boot_mode(now)?;
        let latch = LiveLatch::load(&self.ledger)?;
        let (boot_count, mode_before) = {
            let mut state = self.lock()?;
            let before = state.mode;
            state.mode = mode;
            state.booted = true;
            state.latch = latch;
            state.watchdog.mark(now);
            (state.latch.boot_entries().count(), before)
        };
        self.restore_cells(now).await?;
        self.event(
            now,
            EventLevel::Info,
            "engine.boot",
            format!(
                "mode {} (was {}), {boot_count} setpoint(s) from a previous run",
                mode.slug(),
                mode_before.slug()
            ),
        )?;
        self.retry_boot_latches(now).await?;
        self.publish_boot(now);
        Ok(())
    }

    /// The stored mode, seeding it from the configured default on the very first boot.
    fn boot_mode(&self, now: i64) -> Result<crate::safety::Authority, EngineError> {
        let stored = self.ledger.settings()?.get(crate::mode::MODE_KEY)?;
        if stored.is_some() {
            return Ok(crate::mode::load(&self.ledger)?);
        }
        crate::mode::save(&self.ledger, self.config.mode, now)?;
        Ok(self.config.mode)
    }

    /// Restore the persisted per-pack cell latches for every pack in the fleet.
    async fn restore_cells(&self, now: i64) -> Result<(), EngineError> {
        let view = self.fleet.view(now).await?;
        let mut restored = 0;
        {
            let mut state = self.lock()?;
            for pack in &view.packs {
                match state.cells.restore(&self.cell_store, pack.pack_id()) {
                    Ok(true) => restored += 1,
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(pack = pack.pack_id(), %error, "cell latch restore failed");
                    }
                }
            }
        }
        if restored > 0 {
            self.event(
                now,
                EventLevel::Info,
                "protection.restored",
                format!("{restored} cell-voltage latch(es) restored"),
            )?;
        }
        Ok(())
    }

    /// Neutralise the packs a previous run left live; safe to call repeatedly until the latch is empty.
    async fn retry_boot_latches(&self, now: i64) -> Result<(), EngineError> {
        let pending: Vec<String> = {
            let state = self.lock()?;
            state
                .latch
                .boot_entries()
                .map(|entry| entry.pack_id.clone())
                .collect()
        };
        if pending.is_empty() {
            return Ok(());
        }
        let held = self.hold_all(now, Reason::BootHold, Some(&pending)).await?;
        self.event(
            now,
            EventLevel::Warn,
            "boot.recovered",
            format!(
                "{}/{held} setpoint(s) neutralised",
                pending.len(),
                held = held.len()
            ),
        )?;
        Ok(())
    }

    /// One guard step: hold everything if the loop stopped making progress, then retry boot recovery.
    pub async fn supervise(&self, now: i64) -> Result<bool, EngineError> {
        let (stalled, mode) = {
            let state = self.lock()?;
            (state.watchdog.stalled(now), state.mode)
        };
        let mut held = false;
        if stalled {
            self.lock()?.watchdog.mark(now);
            if mode.may_write() {
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
    /// In `auto` every pack is held; outside it only the packs in the live-write latch are, which is
    /// exactly "hardware we drove". Nothing else is ever written on the way out.
    pub async fn shutdown(&self, now: i64, reason: &str) -> Result<(), EngineError> {
        let (mode, pending) = {
            let state = self.lock()?;
            (state.mode, state.latch.pack_ids())
        };
        let held = if mode.may_write() {
            self.hold_all(now, Reason::Shutdown, None).await?
        } else if pending.is_empty() {
            Vec::new()
        } else {
            self.hold_all(now, Reason::Shutdown, Some(&pending)).await?
        };
        self.event(
            now,
            EventLevel::Info,
            "engine.shutdown",
            format!(
                "{reason}: {}/{n} pack(s) held",
                held.len(),
                n = pending.len()
            ),
        )?;
        Ok(())
    }

    /// Neutralise the latched packs before leaving `auto` (mode change, dry-run flip).
    pub(crate) async fn release(&self, now: i64) -> Result<(), EngineError> {
        let pending = self.lock()?.latch.pack_ids();
        if pending.is_empty() {
            return Ok(());
        }
        self.hold_all(now, Reason::Shutdown, Some(&pending)).await?;
        Ok(())
    }

    /// Hold every pack (or just `only`), recording each attempt and clearing verified holds.
    async fn hold_all(
        &self,
        now: i64,
        reason: Reason,
        only: Option<&[String]>,
    ) -> Result<Vec<String>, EngineError> {
        let ids = self.hold_targets(only)?;
        let mut held = Vec::new();
        for (pack_id, ledger_id) in ids {
            if self.hold_one(&pack_id, ledger_id, now, reason).await? {
                held.push(pack_id);
            }
        }
        Ok(held)
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

    /// One real hold; a verified echo clears the pack's latch.
    async fn hold_one(
        &self,
        pack_id: &str,
        ledger_id: Option<i64>,
        now: i64,
        reason: Reason,
    ) -> Result<bool, EngineError> {
        match self.sink.hold(pack_id).await {
            Ok(receipt) => {
                let mut state = self.lock()?;
                if receipt.verified {
                    state.latch.clear(pack_id);
                }
                state.latch.save(&self.ledger, now)?;
                drop(state);
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
}
