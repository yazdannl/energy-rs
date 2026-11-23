//! One transport call on its worker, arbitrated at completion against its deadline.
//!
//! A request that has left for the device cannot be un-sent. Completion arbitration only
//! protects M4's state: a write that finished before its deadline owns the successful claim,
//! no matter when the collector reaches it. A late completion cannot contaminate later state.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::dispatch::constants::WORKER_TERMINATION_BOUND_MULTIPLIER;
use crate::dispatch::write_claim::Claim;
use crate::dispatch::write_worker::{WorkerArgs, run_worker};
use crate::dispatch::{
    PackCommand, PackIo, WorkerHandle, WorkerTerminationViolation, WriteAck, WriteAttempt,
    WriteTaskContext,
};

/// Shared cleanup state for one write generation.
#[derive(Clone)]
struct Cleanup {
    claim: Arc<Claim>,
    io: Arc<dyn PackIo>,
    pack_id: String,
    tx_id: u64,
    generation: u64,
    current: Arc<AtomicU64>,
    released: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    termination_deadline: Instant,
    bound_ms: u64,
    violations: Arc<Mutex<Vec<WorkerTerminationViolation>>>,
    workers: Arc<Mutex<Vec<WorkerHandle>>>,
}

impl Cleanup {
    fn new(init: CleanupInit<'_>) -> Self {
        Self {
            claim: init.claim,
            io: Arc::clone(init.io),
            pack_id: init.pack_id.to_owned(),
            tx_id: init.tx_id,
            generation: init.generation,
            current: init.current,
            released: init.released,
            done: init.done,
            termination_deadline: init.termination_deadline,
            bound_ms: init.bound_ms,
            violations: Arc::clone(init.violations),
            workers: Arc::clone(init.workers),
        }
    }

    fn release_once(&self) {
        if !self.released.swap(true, Ordering::SeqCst) {
            self.io.release(&self.pack_id, self.tx_id);
        }
    }

    fn supersede(&self) {
        let _ = self.current.compare_exchange(
            self.generation,
            self.generation.saturating_add(1),
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        self.io.cancel(&self.pack_id, self.tx_id);
        self.release_once();
    }

    fn record_violation(&self) {
        let outstanding = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|worker| !worker.done.load(Ordering::SeqCst))
            .count();
        self.violations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(WorkerTerminationViolation {
                pack_id: self.pack_id.clone(),
                generation: self.generation,
                bound_ms: self.bound_ms,
                outstanding,
            });
    }
}

struct CleanupInit<'a> {
    io: &'a Arc<dyn PackIo>,
    pack_id: &'a str,
    tx_id: u64,
    generation: u64,
    current: Arc<AtomicU64>,
    claim: Arc<Claim>,
    released: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    termination_deadline: Instant,
    bound_ms: u64,
    violations: &'a Arc<Mutex<Vec<WorkerTerminationViolation>>>,
    workers: &'a Arc<Mutex<Vec<WorkerHandle>>>,
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn spawn_worker(args: WorkerArgs) -> JoinHandle<()> {
    thread::spawn(move || run_worker(args))
}

fn spawn_watchdog(cleanup: Cleanup, deadline: Instant) {
    thread::spawn(move || {
        cleanup.claim.wait(deadline);
        if cleanup.claim.claim() {
            cleanup.supersede();
        }
        let left = cleanup
            .termination_deadline
            .saturating_duration_since(Instant::now());
        if left > Duration::ZERO {
            thread::sleep(left);
        }
        if !cleanup.done.load(Ordering::SeqCst) {
            cleanup.record_violation();
        }
    });
}

/// A started write; all tasks start before any of them is collected.
pub struct WriteTask {
    receiver: Receiver<WriteAck>,
    worker: Option<JoinHandle<()>>,
    cleanup: Cleanup,
    started: Instant,
    deadline: Instant,
    budget_ms: u64,
}

impl WriteTask {
    /// Start a worker and a deadline watchdog; neither waits for confirmation here.
    pub fn start(
        io: &Arc<dyn PackIo>,
        command: &PackCommand,
        tx_id: u64,
        budget_ms: u64,
        generation: u64,
        context: &WriteTaskContext<'_>,
    ) -> Self {
        let started = Instant::now();
        let deadline = started
            .checked_add(Duration::from_millis(budget_ms))
            .unwrap_or(started);
        let bound_ms = budget_ms.saturating_mul(WORKER_TERMINATION_BOUND_MULTIPLIER);
        let termination_deadline = started
            .checked_add(Duration::from_millis(bound_ms))
            .unwrap_or(deadline);
        let (sender, receiver) = channel::<WriteAck>();
        let current = Arc::new(AtomicU64::new(generation));
        let claim = Arc::new(Claim::default());
        let done = Arc::new(AtomicBool::new(false));
        context
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(WorkerHandle {
                pack_id: command.pack_id.clone(),
                generation,
                done: Arc::clone(&done),
            });
        let worker = spawn_worker(WorkerArgs {
            io: Arc::clone(io),
            command: command.clone(),
            tx_id,
            budget_ms,
            generation,
            late: Arc::clone(context.late),
            claim: Arc::clone(&claim),
            done: Arc::clone(&done),
            sender,
            deadline,
        });
        let cleanup = Cleanup::new(CleanupInit {
            io,
            pack_id: &command.pack_id,
            tx_id,
            generation,
            current,
            claim,
            released: Arc::new(AtomicBool::new(false)),
            done,
            termination_deadline,
            bound_ms,
            violations: context.violations,
            workers: context.workers,
        });
        spawn_watchdog(cleanup.clone(), deadline);
        Self {
            receiver,
            worker: Some(worker),
            cleanup,
            started,
            deadline,
            budget_ms,
        }
    }

    fn completed(&self, ack: WriteAck) -> WriteAttempt {
        self.cleanup.release_once();
        WriteAttempt::Ack {
            ack,
            elapsed_ms: elapsed_ms(self.started),
        }
    }

    /// Collect against this task's absolute deadline; completion is already arbitrated.
    pub fn collect(mut self) -> WriteAttempt {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        match self.receiver.recv_timeout(remaining) {
            Ok(ack) => {
                self.finish_worker();
                self.completed(ack)
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Ok(ack) = self.receiver.try_recv() {
                    self.finish_worker();
                    return self.completed(ack);
                }
                if self.cleanup.claim.claim() {
                    self.cleanup.supersede();
                }
                WriteAttempt::Timeout {
                    elapsed_ms: elapsed_ms(self.started).min(self.budget_ms),
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if self.cleanup.claim.claim() {
                    self.cleanup.supersede();
                }
                WriteAttempt::Failed {
                    reason: "write_worker_failed",
                    elapsed_ms: elapsed_ms(self.started),
                }
            }
        }
    }

    fn finish_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}
