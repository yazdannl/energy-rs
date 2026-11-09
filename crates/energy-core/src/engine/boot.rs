//! Boot: the persisted mode and latch, protection restore and boot neutralisation.

use energy_ledger::EventLevel;

use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::engine::live_latch::LiveLatch;
use crate::safety::{Authority, Reason};

impl Engine {
    /// Load the persisted mode and latch, restore the protection state and neutralise boot setpoints.
    ///
    /// Only packs in the live-write latch are touched, and only in `auto`: outside it the gate refuses
    /// every command, holds included, so a `dry_run` restart cannot write the setpoints a previous
    /// `auto` run left behind -- they stay latched and are neutralised the moment control enters
    /// `auto`. A latch that cannot be read is a recovery error, not an empty latch: the effective
    /// mode drops to `dry_run` and `auto` stays refused until an operator reconciles it.
    pub async fn boot(&self, now: i64) -> Result<(), EngineError> {
        let stored = self.boot_mode(now)?;
        let (mode, latch, corrupt) = self.load_latch(stored)?;
        let boot_count = latch.boot_entries().count();
        let before = self.gate.mode();
        {
            let mut state = self.lock()?;
            state.booted = true;
            state.latch = latch;
            state.latch_corrupt = corrupt;
            state.watchdog.mark(now);
        }
        self.gate.transition(mode);
        self.restore_cells(now).await?;
        self.event(
            now,
            EventLevel::Info,
            "engine.boot",
            format!(
                "mode {} (was {}), {boot_count} setpoint(s) from a previous run",
                mode.slug(),
                before.slug()
            ),
        )?;
        if let Some(bytes) = corrupt {
            self.event(
                now,
                EventLevel::Error,
                "engine.latch_corrupt",
                format!(
                    "the live-write latch ({bytes} bytes) cannot be read; running {} and refusing \
                     auto until it is reconciled",
                    mode.slug()
                ),
            )?;
        } else if boot_count > 0 && !mode.may_write() {
            self.event(
                now,
                EventLevel::Warn,
                "boot.recovery_deferred",
                format!(
                    "{boot_count} setpoint(s) from a previous run stay latched: {} writes nothing",
                    mode.slug()
                ),
            )?;
        }
        self.retry_boot_latches(now).await?;
        self.restore_selection(now)?;
        self.publish_boot(now);
        Ok(())
    }

    /// Load the latched pack selection so a restart does not rotate the pack mid-cycle.
    ///
    /// A hold older than its own bound is dropped instead of trusted: the row is state, not policy,
    /// and a stale row must never pin a pack after a long idle.
    fn restore_selection(&self, now: i64) -> Result<(), EngineError> {
        let stored = self.ledger.selection()?.get()?;
        let Some(row) = stored else {
            return Ok(());
        };
        let age = now.saturating_sub(row.selected_at);
        let mut state = self.lock()?;
        if self.config.pack_hold_seconds <= 0 || age > self.config.pack_hold_seconds {
            self.ledger.selection()?.clear()?;
            state.selection = None;
            return Ok(());
        }
        state.selection = Some(row);
        Ok(())
    }

    /// The stored mode, seeding it from the configured default on the very first boot.
    fn boot_mode(&self, now: i64) -> Result<Authority, EngineError> {
        let stored = self.ledger.settings()?.get(crate::mode::MODE_KEY)?;
        if stored.is_some() {
            return Ok(crate::mode::load(&self.ledger)?);
        }
        crate::mode::save(&self.ledger, self.config.mode, now)?;
        Ok(self.config.mode)
    }

    /// The effective mode, the latch and (when unreadable) the size of the stored value.
    ///
    /// An unreadable latch falls back to `dry_run`, never to the stored `auto`: what the hardware is
    /// doing is unknown, so nothing may be written until an operator accepts the loss.
    fn load_latch(
        &self,
        stored: Authority,
    ) -> Result<(Authority, LiveLatch, Option<usize>), EngineError> {
        match LiveLatch::load(&self.ledger) {
            Ok(latch) => Ok((stored, latch, None)),
            Err(EngineError::LatchCorrupt { bytes }) => {
                Ok((Authority::DryRun, LiveLatch::new(), Some(bytes)))
            }
            Err(error) => Err(error),
        }
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
    pub(crate) async fn retry_boot_latches(&self, now: i64) -> Result<(), EngineError> {
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
}
