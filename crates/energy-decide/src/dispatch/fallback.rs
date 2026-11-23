//! Fail-safe hold retries at 0/2/6 s, all inside the one per-pack budget.

use std::time::Instant;

use crate::dispatch::budget::{elapsed_ms, remaining_ms};
use crate::dispatch::constants::FALLBACK_RETRY_MS;
use crate::dispatch::{
    DispatchContext, FallbackResult, PackCommand, WriteAck, WriteAttempt, WriteTask,
    WriteTaskContext,
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
    let tasks = WriteTaskContext {
        late: context.late,
        violations: context.violations,
        workers: context.workers,
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
        let remaining = remaining_ms(started, budget_ms);
        let attempt =
            WriteTask::start(context.io, &hold, tx_id, remaining, hold.generation, &tasks)
                .collect();
        match attempt {
            WriteAttempt::Ack { ack, .. } => {
                let accepted = ack.accepted();
                result.ack = Some(ack);
                if accepted {
                    break;
                }
            }
            WriteAttempt::Timeout { .. }
            | WriteAttempt::Superseded { .. }
            | WriteAttempt::Failed { .. } => {}
        }
    }
    result.elapsed_ms = elapsed_ms(started);
    result
}

/// Whether a fallback transport answer accepted the hold.
pub fn fallback_accepted(result: &FallbackResult) -> bool {
    matches!(result.ack, Some(WriteAck::Accepted { .. }))
}
