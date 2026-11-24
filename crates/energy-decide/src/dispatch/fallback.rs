//! Fail-safe hold retries at 0/2/6 s, all inside the one per-pack budget.

use std::sync::Arc;
use std::time::Instant;

use crate::dispatch::budget::{elapsed_ms, remaining_ms};
use crate::dispatch::constants::FALLBACK_RETRY_MS;
use crate::dispatch::{
    DispatchContext, DriveContext, FallbackResult, PackCommand, WriteAck, WriteAttempt,
    WriteFuture, drive,
};

fn hold_command(command: &PackCommand) -> PackCommand {
    let mut hold = command.clone();
    hold.requested_w = 0.0;
    hold.safety_traffic = true;
    hold
}

/// Try a neutral hold until the shared budget is exhausted or transport accepts.
pub fn write_fallback(
    context: &DispatchContext<'_>,
    command: &PackCommand,
    budget_ms: u64,
) -> FallbackResult {
    let started = Instant::now();
    let hold = hold_command(command);
    let mut result = FallbackResult {
        ack: None,
        attempts: 0,
        offsets_ms: Vec::new(),
        elapsed_ms: 0,
    };
    let driver = DriveContext {
        io: context.io,
        clock: context.clock,
        late: context.late,
        bound: context.bound,
        max_polls: context.max_polls,
    };
    for offset in FALLBACK_RETRY_MS {
        let left = remaining_ms(started, budget_ms);
        if left == 0 {
            break;
        }
        if let Some(previous) = result.offsets_ms.last() {
            context.clock.sleep_ms(offset.saturating_sub(*previous));
        }
        result.offsets_ms.push(offset);
        result.attempts += 1;
        let tx_id = context.io.begin(&hold.pack_id);
        let guard =
            crate::dispatch::TransactionGuard::new(Arc::clone(context.io), &hold.pack_id, tx_id);
        let future = WriteFuture::start(context.io, &hold, guard, left);
        let outcome = drive(vec![future], &driver);
        if let Some(WriteAttempt::Ack { ack, .. }) = outcome.attempts.into_iter().next() {
            let accepted = ack.accepted();
            result.ack = Some(ack);
            if accepted {
                break;
            }
        }
    }
    result.elapsed_ms = elapsed_ms(started);
    result
}

/// Whether a fallback transport answer accepted the hold.
pub fn fallback_accepted(result: &FallbackResult) -> bool {
    matches!(result.ack, Some(WriteAck::Accepted { .. }))
}
