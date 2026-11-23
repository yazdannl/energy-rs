//! The hold primitives: one bounded, verified hold per pack and its journal.

use std::time::Duration;

use energy_ledger::{DispatchEntry, EventLevel};

use crate::dispatch::{AppliedPowerReason, CommandSink, SinkError};
use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::safety::Reason;

use super::HoldState;

/// Why a hold did not verify, for one journal row.
pub(super) struct UnverifiedHold {
    reason: Reason,
    detail: String,
    applied_w_reason: Option<AppliedPowerReason>,
}

impl UnverifiedHold {
    fn new(
        reason: Reason,
        detail: impl Into<String>,
        applied_w_reason: Option<AppliedPowerReason>,
    ) -> Self {
        Self {
            reason,
            detail: detail.into(),
            applied_w_reason,
        }
    }
}

impl Engine {
    /// One real hold through the gate; a verified echo clears the pack's latch.
    pub(super) async fn hold_one(
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
                self.record_verified(pack_id, ledger_id, now, reason, receipt.applied_w)?;
                Ok(HoldState::Verified)
            }
            Ok(Ok(_)) => {
                self.record_unverified(
                    pack_id,
                    ledger_id,
                    now,
                    UnverifiedHold::new(
                        reason,
                        "hold not echoed",
                        Some(AppliedPowerReason::ReadFailed),
                    ),
                )?;
                Ok(HoldState::Unverified)
            }
            // Authority or generation withdrew the command: nothing was sent and ownership stays.
            Ok(Err(SinkError::Fenced { .. })) => Ok(HoldState::Fenced),
            Ok(Err(error)) => self.record_hold_error(pack_id, ledger_id, now, reason, &error),
            Err(_) => self.record_hold_timeout(pack_id, ledger_id, now, budget),
        }
    }

    fn record_verified(
        &self,
        pack_id: &str,
        ledger_id: Option<i64>,
        now: i64,
        reason: Reason,
        applied_w: f64,
    ) -> Result<(), EngineError> {
        self.clear_owned(pack_id, now)?;
        let entry = DispatchEntry::new(now, ledger_id, 0.0, applied_w, reason.as_str(), "auto")
            .verified(applied_w)
            .flow_measured(true);
        self.ledger.dispatch()?.insert(&entry)?;
        Ok(())
    }

    /// A failed hold is still a dispatch attempt we own: journal it instead of leaving a silent gap.
    /// A missing transport is not a device refusal and gets its own machine-readable reason.
    fn record_hold_error(
        &self,
        pack_id: &str,
        ledger_id: Option<i64>,
        now: i64,
        reason: Reason,
        error: &SinkError,
    ) -> Result<HoldState, EngineError> {
        let cause = error.to_string();
        let unreachable = matches!(
            error,
            SinkError::DeviceUnreachable { .. } | SinkError::ReadUnverified { .. }
        );
        let row_reason = if unreachable {
            Reason::HoldUnverifiableDeviceUnreachable
        } else {
            reason
        };
        self.event(
            now,
            EventLevel::Error,
            "hold.failed",
            format!("{pack_id}: {cause}"),
        )?;
        let mut entry = DispatchEntry::new(now, ledger_id, 0.0, 0.0, row_reason.as_str(), "auto")
            .failed(format!("hold failed: {cause}"));
        if unreachable {
            entry = entry.with_applied_w_reason(AppliedPowerReason::ReadFailed.as_str());
        }
        self.ledger.dispatch()?.insert(&entry)?;
        Ok(HoldState::Unverified)
    }

    fn record_hold_timeout(
        &self,
        pack_id: &str,
        ledger_id: Option<i64>,
        now: i64,
        budget: Duration,
    ) -> Result<HoldState, EngineError> {
        let detail = format!(
            "hold not verified within the {}s budget; device did not answer",
            budget.as_secs()
        );
        self.record_unverified(
            pack_id,
            ledger_id,
            now,
            UnverifiedHold::new(
                Reason::HoldUnverifiableDeviceUnreachable,
                detail,
                Some(AppliedPowerReason::ReadFailed),
            ),
        )?;
        Ok(HoldState::Unverified)
    }

    /// Journal an unverified hold by pack, rather than one aggregate shutdown warning.
    ///
    /// The dispatch row stays unverified and the latch is left alone: a write we cannot confirm
    /// still owns the pack.
    pub(super) fn record_unverified(
        &self,
        pack_id: &str,
        ledger_id: Option<i64>,
        now: i64,
        verdict: UnverifiedHold,
    ) -> Result<(), EngineError> {
        self.event(
            now,
            EventLevel::Warn,
            "hold.unverified",
            format!("{pack_id}: {}", verdict.detail),
        )?;
        let mut entry =
            DispatchEntry::new(now, ledger_id, 0.0, 0.0, verdict.reason.as_str(), "auto")
                .failed(verdict.detail);
        if let Some(applied_reason) = verdict.applied_w_reason {
            entry = entry.with_applied_w_reason(applied_reason.as_str());
        }
        self.ledger.dispatch()?.insert(&entry)?;
        Ok(())
    }

    /// Drop a pack's ownership durably, and only after a verified hold.
    ///
    /// The new latch is persisted first and committed in memory only when the write succeeded, so a
    /// ledger failure can never lose track of a pack that was actually driven.
    pub(super) fn clear_owned(&self, pack_id: &str, now: i64) -> Result<(), EngineError> {
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
