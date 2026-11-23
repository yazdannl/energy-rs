//! One transport call on its own worker, bounded by M4's budget.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::dispatch::{PackCommand, PackIo, WriteAck, WriteAttempt};

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// A started write; the fleet starts all tasks before collecting any of them.
#[derive(Debug)]
pub struct WriteTask {
    receiver: Receiver<WriteAck>,
    worker: JoinHandle<()>,
    started: Instant,
    budget_ms: u64,
}

impl WriteTask {
    /// Start a worker and return immediately; no confirmation is awaited here.
    pub fn start(io: &Arc<dyn PackIo>, command: &PackCommand, _tx_id: u64, budget_ms: u64) -> Self {
        let (sender, receiver): (Sender<WriteAck>, Receiver<WriteAck>) = channel();
        let worker_io = Arc::clone(io);
        let worker_command = command.clone();
        let worker = thread::spawn(move || {
            let ack = worker_io.write(&worker_command.pack_id, &worker_command, budget_ms);
            drop(sender.send(ack));
        });
        Self {
            receiver,
            worker,
            started: Instant::now(),
            budget_ms,
        }
    }

    /// Wait at most the single end-to-end budget for this pack's transport.
    pub fn collect(self) -> WriteAttempt {
        let wait = Duration::from_millis(self.budget_ms);
        match self.receiver.recv_timeout(wait) {
            Ok(ack) => {
                drop(self.worker.join());
                WriteAttempt::Ack {
                    ack,
                    elapsed_ms: elapsed_ms(self.started),
                }
            }
            Err(RecvTimeoutError::Timeout) => WriteAttempt::Timeout {
                elapsed_ms: elapsed_ms(self.started),
            },
            Err(RecvTimeoutError::Disconnected) => WriteAttempt::Failed {
                reason: "write_worker_failed",
                elapsed_ms: elapsed_ms(self.started),
            },
        }
    }
}
