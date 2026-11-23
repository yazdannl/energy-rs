//! Honest row for a missing flow observation.

use crate::dispatch::{
    AppliedReason, DispatchOutcome, DispatchRow, DispatchState, EvidenceKind, PackCommand,
};

pub fn row(state: &DispatchState, pack_id: &str, now_ms: i64) -> DispatchRow {
    let pack = state.get(pack_id);
    let command = pack
        .and_then(|value| value.last_command.clone())
        .unwrap_or(PackCommand {
            pack_id: pack_id.to_owned(),
            requested_w: 0.0,
            issued_at_ms: now_ms,
            generation: 0,
            fence_id: None,
            safety_traffic: false,
        });
    let uncertain = pack.is_some_and(|value| value.uncertain);
    let pending = pack.is_some_and(|value| value.pending.is_some());
    let mut row = DispatchRow::base(&command, crate::dispatch::constants::PACK_IO_BUDGET_MS)
        .outcome(
            if uncertain {
                DispatchOutcome::Uncertain
            } else if pending {
                DispatchOutcome::ConfirmationPending
            } else {
                DispatchOutcome::Deferred
            },
            EvidenceKind::NoMeasurement,
        )
        .reason("read_miss");
    row.applied_reason = AppliedReason::ReadMiss;
    row.controller_uncertain = uncertain;
    if uncertain {
        row = row.reason("controller_uncertain");
    }
    if !pending && !uncertain {
        row = row.reason("no_pending_command");
    }
    row
}
