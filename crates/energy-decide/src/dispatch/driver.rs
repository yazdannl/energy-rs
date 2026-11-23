//! One arbiter drives every pack future; completion is decided here only.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::dispatch::constants::DRIVE_POLL_INTERVAL_MS;
use crate::dispatch::{DriveBoundEvent, DriveContext, LateWrite, WriteAttempt, WriteFuture};

/// Bounded write results plus futures the arbiter deliberately left unresolved.
pub struct DriveResult {
    /// One attempt result per started future, in submission order.
    pub attempts: Vec<WriteAttempt>,
    /// Futures still inside transport after their deadline; never restarted.
    pub unresolved: Vec<WriteFuture>,
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

fn finish(
    slots: &mut [Option<WriteFuture>],
    index: usize,
    context: &DriveContext<'_>,
    attempts: &mut [Option<WriteAttempt>],
) {
    let Some(future) = slots[index].take() else {
        return;
    };
    let elapsed = future.elapsed_ms();
    let deadline = future.deadline();
    let generation = future.generation();
    let pack_id = future.pack_id().to_owned();
    future.release_once(context.io);
    let result = future.join();
    attempts[index] = Some(if result.completed_at > deadline {
        record_late(context.late, &pack_id, generation);
        WriteAttempt::Superseded {
            generation,
            elapsed_ms: elapsed,
        }
    } else {
        WriteAttempt::Ack {
            ack: result.ack,
            elapsed_ms: elapsed,
        }
    });
}

fn expire(
    slots: &mut [Option<WriteFuture>],
    index: usize,
    context: &DriveContext<'_>,
    attempts: &mut [Option<WriteAttempt>],
    unresolved: &mut Vec<WriteFuture>,
) {
    let Some(future) = slots[index].take() else {
        return;
    };
    future.cancel_and_release(context.io);
    attempts[index] = Some(WriteAttempt::Timeout {
        elapsed_ms: future.elapsed_ms(),
    });
    unresolved.push(future);
}

/// Drive all futures with one loop; no channel, claim arbitration or watchdog exists.
pub fn drive(futures: Vec<WriteFuture>, context: &DriveContext<'_>) -> DriveResult {
    let mut slots: Vec<Option<WriteFuture>> = futures.into_iter().map(Some).collect();
    let mut attempts: Vec<Option<WriteAttempt>> = vec![None; slots.len()];
    let mut unresolved = Vec::new();
    let mut polls = 0;
    while slots.iter().any(Option::is_some) {
        polls += 1;
        if polls > context.max_polls {
            if let Some(future) = slots.iter().flatten().next() {
                record_bound(context, future, polls);
            }
            for future in slots.iter().flatten() {
                future.cancel_and_release(context.io);
            }
            break;
        }
        for index in 0..slots.len() {
            let now = Instant::now();
            if slots[index].as_ref().is_some_and(WriteFuture::finished) {
                finish(&mut slots, index, context, &mut attempts);
            } else if slots[index]
                .as_ref()
                .is_some_and(|future| now >= future.deadline())
            {
                expire(&mut slots, index, context, &mut attempts, &mut unresolved);
            }
        }
        if slots.iter().any(Option::is_some) {
            thread::sleep(Duration::from_millis(DRIVE_POLL_INTERVAL_MS));
        }
    }
    for (index, slot) in slots.into_iter().enumerate() {
        match slot {
            Some(future) => {
                let generation = future.generation();
                let elapsed = future.elapsed_ms();
                attempts[index] = Some(WriteAttempt::BoundExceeded {
                    generation,
                    elapsed_ms: elapsed,
                });
                unresolved.push(future);
            }
            None if attempts[index].is_none() => {
                attempts[index] = Some(WriteAttempt::Failed {
                    reason: "future_missing",
                    elapsed_ms: 0,
                });
            }
            None => {}
        }
    }
    DriveResult {
        attempts: attempts.into_iter().flatten().collect(),
        unresolved,
    }
}
