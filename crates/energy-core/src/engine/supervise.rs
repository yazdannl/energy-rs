//! The guard step: the watchdog hold, release, shutdown and the hold primitive.

use std::time::Duration;

use energy_battery::control::{ECHO_CONFIRM_WINDOW, MIN_WRITE_SPACING};
use energy_ledger::{DispatchEntry, EventLevel};
use futures_util::future::join_all;

use crate::dispatch::{CommandSink, SinkError};
use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::engine::hold_report::HoldReport;
use crate::safety::Reason;

/// The budget for one pack's verified hold: a device pacer slot, one echo window and scheduling
/// slack. Holds are fanned out across independent devices, so the fleet costs one budget, not one
/// budget per pack.
pub const PACK_HOLD_BUDGET: Duration =
    Duration::from_secs(MIN_WRITE_SPACING.as_secs() + ECHO_CONFIRM_WINDOW.as_secs() + 2);

/// One pack's hold outcome: only `Verified` may be counted as held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldState {
    Verified,
    Unverified,
    Fenced,
}

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
            let mut detail = format!("nothing written in {} mode", mode.slug());
            if mode.may_write() {
                self.gate.fence();
                let report = self.hold_all(now, Reason::WatchdogHold, None).await?;
                held = true;
                detail = report.summary();
            }
            self.event(
                now,
                EventLevel::Error,
                "watchdog.hold",
                format!(
                    "no control progress for {} s; {detail}",
                    self.config.stall_after_s()
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
        self.shutdown_within(now, reason, PACK_HOLD_BUDGET)
            .await
            .map(|_| ())
    }

    /// The same shutdown, with the hold budget taken from the caller's absolute deadline.
    pub(crate) async fn shutdown_within(
        &self,
        now: i64,
        reason: &str,
        budget: Duration,
    ) -> Result<HoldReport, EngineError> {
        let mode = self.gate.mode();
        let pending = self.lock()?.latch.pack_ids().len();
        self.gate.fence();
        let report = if mode.may_write() {
            self.hold_all_within(now, Reason::Shutdown, None, budget)
                .await?
        } else {
            HoldReport::default()
        };
        let detail = if mode.may_write() {
            report.summary()
        } else {
            format!(
                "nothing written in {} mode, {pending} latched pack(s) kept",
                mode.slug()
            )
        };
        let level = if report.unverified().is_empty() && report.fenced().is_empty() {
            EventLevel::Info
        } else {
            EventLevel::Warn
        };
        self.event(now, level, "engine.shutdown", format!("{reason}: {detail}"))?;
        Ok(report)
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
    /// Devices are independent: one fan-out sends all holds in parallel while each device keeps its
    /// own pacer and one write in flight. A budget bounds each pack's wait, and every target is
    /// attempted even when one times out or its bookkeeping fails; the failures are returned
    /// together, and the packs they concern keep their latch entry.
    pub(crate) async fn hold_all(
        &self,
        now: i64,
        reason: Reason,
        only: Option<&[String]>,
    ) -> Result<HoldReport, EngineError> {
        self.hold_all_within(now, reason, only, PACK_HOLD_BUDGET)
            .await
    }

    /// Fan out every hold under one budget, keeping the three outcomes separate.
    pub(crate) async fn hold_all_within(
        &self,
        now: i64,
        reason: Reason,
        only: Option<&[String]>,
        budget: Duration,
    ) -> Result<HoldReport, EngineError> {
        let attempts =
            self.hold_targets(only)?
                .into_iter()
                .map(|(pack_id, ledger_id)| async move {
                    let outcome = self
                        .hold_one(&pack_id, ledger_id, now, reason, budget)
                        .await;
                    (pack_id, outcome)
                });
        let mut verified = Vec::new();
        let mut unverified = Vec::new();
        let mut fenced = Vec::new();
        let mut failures = Vec::new();
        for (pack_id, outcome) in join_all(attempts).await {
            match outcome {
                Ok(HoldState::Verified) => verified.push(pack_id),
                Ok(HoldState::Unverified) => unverified.push(pack_id),
                Ok(HoldState::Fenced) => fenced.push(pack_id),
                Err(error) => failures.push(format!("{pack_id}: {error}")),
            }
        }
        if !failures.is_empty() {
            return Err(EngineError::Holds {
                failed: failures.len(),
                detail: failures.join("; "),
            });
        }
        Ok(HoldReport::new(verified, unverified, fenced))
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
        budget: Duration,
    ) -> Result<HoldState, EngineError> {
        let token = self.gate.token();
        let sink = self.guarded(token, now);
        let call = sink.hold(pack_id);
        match tokio::time::timeout(budget, call).await {
            Ok(Ok(receipt)) if receipt.verified => {
                self.clear_owned(pack_id, now)?;
                let entry = DispatchEntry::new(
                    now,
                    ledger_id,
                    0.0,
                    receipt.applied_w,
                    reason.as_str(),
                    "auto",
                )
                .verified(receipt.applied_w);
                self.ledger.dispatch()?.insert(&entry)?;
                Ok(HoldState::Verified)
            }
            Ok(Ok(_)) => {
                self.record_unverified(pack_id, ledger_id, now, reason, "hold not echoed")?;
                Ok(HoldState::Unverified)
            }
            // Authority or generation withdrew the command: nothing was sent and ownership stays.
            Ok(Err(SinkError::Fenced { .. })) => Ok(HoldState::Fenced),
            Ok(Err(error)) => {
                self.event(
                    now,
                    EventLevel::Error,
                    "hold.failed",
                    format!("{pack_id}: {error}"),
                )?;
                Ok(HoldState::Unverified)
            }
            Err(_) => {
                let detail = format!("hold not verified within the {}s budget", budget.as_secs());
                self.record_unverified(pack_id, ledger_id, now, reason, &detail)?;
                Ok(HoldState::Unverified)
            }
        }
    }

    /// Journal an unverified hold by pack, rather than one aggregate shutdown warning.
    ///
    /// The dispatch row stays unverified and the latch is left alone: a write we cannot confirm
    /// still owns the pack.
    fn record_unverified(
        &self,
        pack_id: &str,
        ledger_id: Option<i64>,
        now: i64,
        reason: Reason,
        detail: &str,
    ) -> Result<(), EngineError> {
        self.event(
            now,
            EventLevel::Warn,
            "hold.unverified",
            format!("{pack_id}: {detail}"),
        )?;
        let entry =
            DispatchEntry::new(now, ledger_id, 0.0, 0.0, reason.as_str(), "auto").failed(detail);
        self.ledger.dispatch()?.insert(&entry)?;
        Ok(())
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
