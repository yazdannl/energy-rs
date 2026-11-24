//! One per-pack future whose return value is the transport acknowledgement.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::dispatch::{PackCommand, PackIo, ReleaseQueue, TerminalState, WriteAck, WriteResult};

/// A bounded write future owned by the single arbiter.
pub struct WriteFuture {
    pack_id: String,
    generation: u64,
    tx_id: u64,
    started: Instant,
    deadline: Instant,
    budget_ms: u64,
    cancel: Arc<AtomicBool>,
    late_logged: Arc<AtomicBool>,
    terminal: Arc<TerminalState>,
    handle: Option<JoinHandle<()>>,
}

impl WriteFuture {
    /// Spawn one future; no channel, claim or watchdog is created.
    pub fn start(io: &Arc<dyn PackIo>, command: &PackCommand, tx_id: u64, budget_ms: u64) -> Self {
        let started = Instant::now();
        let deadline = started
            .checked_add(Duration::from_millis(budget_ms))
            .unwrap_or(started);
        let cancel = Arc::new(AtomicBool::new(false));
        let terminal = Arc::new(TerminalState::default());
        let worker_io = Arc::clone(io);
        let worker_command = command.clone();
        let worker_cancel = Arc::clone(&cancel);
        let worker_terminal = Arc::clone(&terminal);
        let handle = thread::spawn(move || {
            let ack = worker_io.write(
                &worker_command.pack_id,
                &worker_command,
                budget_ms,
                tx_id,
                worker_cancel,
            );
            let result = WriteResult {
                ack,
                completed_at: Instant::now(),
            };
            worker_terminal.publish(result, deadline);
        });
        Self {
            pack_id: command.pack_id.clone(),
            generation: command.generation,
            tx_id,
            started,
            deadline,
            budget_ms,
            cancel,
            late_logged: Arc::new(AtomicBool::new(false)),
            terminal,
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

    /// Transaction released by this future.
    pub fn tx_id(&self) -> u64 {
        self.tx_id
    }

    /// Absolute deadline of this write.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Shared terminal state, exposed only for deterministic interleaving tests.
    pub fn terminal_state(&self) -> &Arc<TerminalState> {
        &self.terminal
    }

    /// Decide completion from recorded time; never from `JoinHandle` readiness.
    pub fn settle(&self, now: Instant) -> Option<crate::dispatch::WriteTerminal> {
        self.terminal.settle(now, self.deadline)
    }

    /// Elapsed milliseconds since the write started, capped at its budget.
    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis())
            .unwrap_or(u64::MAX)
            .min(self.budget_ms)
    }

    /// Claim the one late-completion log entry for this future.
    pub fn claim_late_log(&self) -> bool {
        !self.late_logged.swap(true, Ordering::SeqCst)
    }

    /// Cancel transport and enqueue release without blocking the arbiter.
    pub fn cancel_and_release(&self, release: &ReleaseQueue) {
        self.cancel.store(true, Ordering::SeqCst);
        release.release(&self.pack_id, self.tx_id);
    }

    /// Whether the OS thread is done. Used only for reaping, never to classify a write.
    pub fn handle_finished(&self) -> bool {
        self.handle.as_ref().is_some_and(JoinHandle::is_finished)
    }

    /// Reap an OS handle already known to be done. This does not classify completion;
    /// callers must check `handle_finished` first and keep unfinished futures pending.
    pub fn reap_finished(mut self) -> WriteResult {
        let terminal = Arc::clone(&self.terminal);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        match terminal.settle(self.deadline, self.deadline) {
            Some(crate::dispatch::WriteTerminal::Completed(result)) => result,
            _ => WriteResult {
                ack: WriteAck::Failed {
                    reason: "write_future_panicked",
                    at_ms: 0,
                },
                completed_at: Instant::now(),
            },
        }
    }
}
