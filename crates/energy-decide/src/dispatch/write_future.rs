//! One per-pack future whose return value is the transport acknowledgement.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::dispatch::{PackCommand, PackIo, WriteAck, WriteResult};

/// A bounded write future owned by the single arbiter.
pub struct WriteFuture {
    pack_id: String,
    generation: u64,
    tx_id: u64,
    started: Instant,
    deadline: Instant,
    budget_ms: u64,
    cancel: Arc<AtomicBool>,
    release_once: Arc<AtomicBool>,
    handle: Option<JoinHandle<WriteResult>>,
}

impl WriteFuture {
    /// Spawn one future; no channel, claim or watchdog is created.
    pub fn start(io: &Arc<dyn PackIo>, command: &PackCommand, tx_id: u64, budget_ms: u64) -> Self {
        let started = Instant::now();
        let deadline = started
            .checked_add(Duration::from_millis(budget_ms))
            .unwrap_or(started);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_io = Arc::clone(io);
        let worker_command = command.clone();
        let worker_cancel = Arc::clone(&cancel);
        let handle = thread::spawn(move || {
            let ack = worker_io.write(
                &worker_command.pack_id,
                &worker_command,
                budget_ms,
                tx_id,
                worker_cancel,
            );
            WriteResult {
                ack,
                completed_at: Instant::now(),
            }
        });
        Self {
            pack_id: command.pack_id.clone(),
            generation: command.generation,
            tx_id,
            started,
            deadline,
            budget_ms,
            cancel,
            release_once: Arc::new(AtomicBool::new(false)),
            handle: Some(handle),
        }
    }

    /// Pack that owns this future.
    pub fn pack_id(&self) -> &str {
        &self.pack_id
    }

    /// Write generation carried by the command.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Absolute deadline of this write.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Whether the transport future has returned.
    pub fn finished(&self) -> bool {
        self.handle.as_ref().is_some_and(JoinHandle::is_finished)
    }

    /// Elapsed milliseconds since the write started, capped at its budget.
    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis())
            .unwrap_or(u64::MAX)
            .min(self.budget_ms)
    }

    /// Request cancellation and release the transaction exactly once.
    pub fn cancel_and_release(&self, io: &Arc<dyn PackIo>) {
        self.cancel.store(true, Ordering::SeqCst);
        self.release_once(io);
    }

    /// Release the transaction exactly once; called when the future returns.
    pub fn release_once(&self, io: &Arc<dyn PackIo>) {
        if !self.release_once.swap(true, Ordering::SeqCst) {
            io.release(&self.pack_id, self.tx_id);
        }
    }

    /// Take the completion value; called only after `finished`.
    pub fn join(mut self) -> WriteResult {
        let completed_at = Instant::now();
        let failed = || WriteResult {
            ack: WriteAck::Failed {
                reason: "write_future_panicked",
                at_ms: 0,
            },
            completed_at,
        };
        self.handle
            .take()
            .map_or_else(failed, |handle| handle.join().unwrap_or_else(|_| failed()))
    }
}
