//! One transport call on its worker, bounded by that write's absolute deadline.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::dispatch::{LateWrite, PackCommand, PackIo, WriteAck, WriteAttempt};

const LATE_REASON: &str = "late_after_deadline_superseded";

/// Shared claim used by the collector and the deadline watchdog.
#[derive(Debug, Default)]
struct Claim {
    done: Mutex<bool>,
    changed: Condvar,
}

impl Claim {
    /// Exactly one caller owns cleanup for one write generation.
    fn claim(&self) -> bool {
        let mut done = self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *done {
            return false;
        }
        *done = true;
        self.changed.notify_all();
        true
    }

    /// Wait until the absolute deadline, or until the collector finished early.
    fn wait(&self, deadline: Instant) {
        let mut done = self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*done {
            let now = Instant::now();
            if now >= deadline {
                return;
            }
            let (guard, result) = self
                .changed
                .wait_timeout(done, deadline.saturating_duration_since(now))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            done = guard;
            if result.timed_out() {
                return;
            }
        }
    }
}

/// Everything needed to cancel, release and supersede one write exactly once.
#[derive(Clone)]
struct Cleanup {
    claim: Arc<Claim>,
    io: Arc<dyn PackIo>,
    pack_id: String,
    tx_id: u64,
    generation: u64,
    current: Arc<AtomicU64>,
}

impl Cleanup {
    /// Bump the generation, stop the transport and release the transaction.
    fn supersede(&self) {
        let _ = self.current.compare_exchange(
            self.generation,
            self.generation.saturating_add(1),
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        self.io.cancel(&self.pack_id, self.tx_id);
        self.io.release(&self.pack_id, self.tx_id);
    }
}

struct WorkerArgs {
    io: Arc<dyn PackIo>,
    command: PackCommand,
    tx_id: u64,
    budget_ms: u64,
    generation: u64,
    current: Arc<AtomicU64>,
    late: Arc<Mutex<Vec<LateWrite>>>,
    sender: Sender<WriteAck>,
}

fn run_worker(args: WorkerArgs) {
    let ack = args.io.write(
        &args.command.pack_id,
        &args.command,
        args.budget_ms,
        args.tx_id,
    );
    if args.current.load(Ordering::SeqCst) != args.generation {
        let mut log = args
            .late
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        log.push(LateWrite {
            pack_id: args.command.pack_id,
            generation: args.generation,
            reason: LATE_REASON,
        });
        return;
    }
    drop(args.sender.send(ack));
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// A started write; all tasks are started before any of them is collected.
pub struct WriteTask {
    receiver: Receiver<WriteAck>,
    worker: Option<JoinHandle<()>>,
    cleanup: Cleanup,
    started: Instant,
    deadline: Instant,
    budget_ms: u64,
}

impl WriteTask {
    /// Start a worker and its deadline watchdog; neither awaits confirmation here.
    pub fn start(
        io: &Arc<dyn PackIo>,
        command: &PackCommand,
        tx_id: u64,
        budget_ms: u64,
        generation: u64,
        late: &Arc<Mutex<Vec<LateWrite>>>,
    ) -> Self {
        let started = Instant::now();
        let (sender, receiver) = channel::<WriteAck>();
        let current = Arc::new(AtomicU64::new(generation));
        let worker_io = Arc::clone(io);
        let worker_command = command.clone();
        let worker_current = Arc::clone(&current);
        let worker_late = Arc::clone(late);
        let worker = thread::spawn(move || {
            run_worker(WorkerArgs {
                io: worker_io,
                command: worker_command,
                tx_id,
                budget_ms,
                generation,
                current: worker_current,
                late: worker_late,
                sender,
            });
        });
        let cleanup = Cleanup {
            claim: Arc::new(Claim::default()),
            io: Arc::clone(io),
            pack_id: command.pack_id.clone(),
            tx_id,
            generation,
            current,
        };
        let watchdog = cleanup.clone();
        let deadline = started
            .checked_add(Duration::from_millis(budget_ms))
            .unwrap_or(started);
        thread::spawn(move || {
            watchdog.claim.wait(deadline);
            if watchdog.claim.claim() {
                watchdog.supersede();
            }
        });
        Self {
            receiver,
            worker: Some(worker),
            cleanup,
            started,
            deadline,
            budget_ms,
        }
    }

    /// Collect against this task's absolute deadline, never a fresh full budget.
    pub fn collect(self) -> WriteAttempt {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        match self.receiver.recv_timeout(remaining) {
            Ok(ack) => {
                if !self.cleanup.claim.claim() {
                    return WriteAttempt::Superseded {
                        generation: self.cleanup.generation,
                        elapsed_ms: elapsed_ms(self.started),
                    };
                }
                if let Some(worker) = self.worker {
                    drop(worker.join());
                }
                self.cleanup
                    .io
                    .release(&self.cleanup.pack_id, self.cleanup.tx_id);
                WriteAttempt::Ack {
                    ack,
                    elapsed_ms: elapsed_ms(self.started),
                }
            }
            Err(RecvTimeoutError::Timeout) => {
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
}
