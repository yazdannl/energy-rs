//! The hold primitives: one bounded, verified hold per pack and its journal.

use std::time::Duration;

use energy_ledger::{DispatchEntry, EventLevel};

use crate::dispatch::{CommandSink, SinkError};
use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::safety::Reason;

use super::HoldState;

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
                // A hold that never landed is still a dispatch attempt we own: journal it with
                // its cause and the pack's ledger id, instead of leaving a silent gap.
                let cause = error.to_string();
                self.event(
                    now,
                    EventLevel::Error,
                    "hold.failed",
                    format!("{pack_id}: {cause}"),
                )?;
                let entry = DispatchEntry::new(now, ledger_id, 0.0, 0.0, reason.as_str(), "auto")
                    .failed(format!("hold failed: {cause}"));
                self.ledger.dispatch()?.insert(&entry)?;
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
    pub(super) fn record_unverified(
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
