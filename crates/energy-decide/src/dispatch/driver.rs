//! One arbiter drives every pack future; completion is decided here only.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::dispatch::constants::DRIVE_POLL_INTERVAL_MS;
use crate::dispatch::{
    DriveBoundEvent, DriveContext, LateWrite, WriteAttempt, WriteFuture, WriteTerminal,
};

/// Bounded write results plus futures the arbiter deliberately left unresolved.
pub struct DriveResult {
    /// One attempt result per started future, in submission order.
    pub attempts: Vec<WriteAttempt>,
    /// Futures still in transport after expiry or bound; never restarted.
    pub unresolved: Vec<WriteFuture>,
    /// Completed futures kept only for nonblocking thread reaping.
    pub retired: Vec<WriteFuture>,
}

pub(crate) fn record_late(late: &Arc<Mutex<Vec<LateWrite>>>, pack_id: &str, generation: u64) {
    late.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(LateWrite {
            pack_id: pack_id.to_owned(),
            generation,
            reason: "late_after_deadline_superseded",
        });
}

struct Sink<'a> {
    attempts: &'a mut [Option<WriteAttempt>],
    unresolved: &'a mut Vec<WriteFuture>,
    retired: &'a mut Vec<WriteFuture>,
}

fn record_bound(context: &DriveContext<'_>, future: &WriteFuture, polls: u64) {
    context
        .bound
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(DriveBoundEvent {
            pack_id: future.pack_id().to_owned(),
            generation: future.generation(),
            polls,
        });
}

/// Turn the one terminal decision into an attempt. The OS handle is never consulted.
fn resolve(
    future: WriteFuture,
    terminal: WriteTerminal,
    context: &DriveContext<'_>,
    at_bound: bool,
) -> (WriteAttempt, WriteFuture) {
    let elapsed = future.elapsed_ms();
    let deadline = future.deadline();
    let generation = future.generation();
    let pack_id = future.pack_id().to_owned();
    let attempt = match terminal {
        WriteTerminal::Completed(result) if result.completed_at <= deadline => WriteAttempt::Ack {
            ack: result.ack,
            elapsed_ms: elapsed,
        },
        WriteTerminal::Completed(_) => {
            if future.claim_late_log() {
                record_late(context.late, &pack_id, generation);
            }
            WriteAttempt::Superseded {
                generation,
                elapsed_ms: elapsed,
            }
        }
        WriteTerminal::Expired | WriteTerminal::Pending if at_bound => {
            WriteAttempt::BoundExceeded {
                generation,
                elapsed_ms: elapsed,
            }
        }
        WriteTerminal::Expired | WriteTerminal::Pending => WriteAttempt::Timeout {
            elapsed_ms: elapsed,
        },
    };
    future.cancel_and_release(context.release);
    (attempt, future)
}

/// Decide and remove one slot; the future is kept only for nonblocking thread reaping.
fn settle(
    slots: &mut [Option<WriteFuture>],
    index: usize,
    context: &DriveContext<'_>,
    sink: &mut Sink<'_>,
    at_bound: bool,
) {
    let Some(future) = slots[index].take() else {
        return;
    };
    let terminal = future
        .settle(Instant::now())
        .unwrap_or(WriteTerminal::Pending);
    let completed = matches!(terminal, WriteTerminal::Completed(_));
    let (attempt, future) = resolve(future, terminal, context, at_bound);
    sink.attempts[index] = Some(attempt);
    if completed {
        sink.retired.push(future);
    } else {
        sink.unresolved.push(future);
    }
}

fn handle_bound(
    slots: &mut [Option<WriteFuture>],
    context: &DriveContext<'_>,
    sink: &mut Sink<'_>,
    polls: u64,
) {
    if let Some(future) = slots.iter().flatten().next() {
        record_bound(context, future, polls);
    }
    for index in 0..slots.len() {
        settle(slots, index, context, sink, true);
    }
}

fn poll_ready(slots: &mut [Option<WriteFuture>], context: &DriveContext<'_>, sink: &mut Sink<'_>) {
    for index in 0..slots.len() {
        if slots[index]
            .as_ref()
            .and_then(|future| future.settle(Instant::now()))
            .is_some()
        {
            settle(slots, index, context, sink, false);
        }
    }
}

/// Drive all futures with one loop; no channel, claim arbitration or watchdog exists.
pub fn drive(futures: Vec<WriteFuture>, context: &DriveContext<'_>) -> DriveResult {
    let mut slots: Vec<Option<WriteFuture>> = futures.into_iter().map(Some).collect();
    let mut attempts: Vec<Option<WriteAttempt>> = vec![None; slots.len()];
    let mut unresolved = Vec::new();
    let mut retired = Vec::new();
    let mut sink = Sink {
        attempts: &mut attempts,
        unresolved: &mut unresolved,
        retired: &mut retired,
    };
    let mut polls = 0;
    while slots.iter().any(Option::is_some) {
        polls += 1;
        if polls > context.max_polls {
            handle_bound(&mut slots, context, &mut sink, polls);
            break;
        }
        poll_ready(&mut slots, context, &mut sink);
        if slots.iter().any(Option::is_some) {
            thread::sleep(Duration::from_millis(DRIVE_POLL_INTERVAL_MS));
        }
    }
    for (index, slot) in slots.into_iter().enumerate() {
        if let Some(future) = slot {
            let generation = future.generation();
            let elapsed = future.elapsed_ms();
            sink.attempts[index] = Some(WriteAttempt::BoundExceeded {
                generation,
                elapsed_ms: elapsed,
            });
            future.cancel_and_release(context.release);
            sink.unresolved.push(future);
        }
    }
    DriveResult {
        attempts: attempts.into_iter().flatten().collect(),
        unresolved,
        retired,
    }
}
