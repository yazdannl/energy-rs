//! Non-blocking transaction release for the single arbiter.
//!
//! `PackIo::release` is a consumer implementation and may block. Dispatch therefore
//! never calls it from the arbiter: release jobs are queued under a mutex and a small
//! executor spawns one short-lived thread per job, so a slow release cannot delay any
//! other pack. This is neither a channel nor a watchdog; no timing decision is made
//! here. The port contract still requires every release call to terminate.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::dispatch::PackIo;

struct ReleaseJob {
    pack_id: String,
    tx_id: u64,
}

/// Queue that keeps release calls off the arbiter thread.
pub struct ReleaseQueue {
    jobs: Arc<Mutex<VecDeque<ReleaseJob>>>,
    changed: Arc<Condvar>,
    stop: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
    executor: Mutex<Option<JoinHandle<()>>>,
}

impl ReleaseQueue {
    /// Start the release executor over a shared transport.
    pub fn new(io: Arc<dyn PackIo>) -> Self {
        let jobs = Arc::new(Mutex::new(VecDeque::<ReleaseJob>::new()));
        let changed = Arc::new(Condvar::new());
        let stop = Arc::new(AtomicBool::new(false));
        let pending = Arc::new(AtomicUsize::new(0));
        let worker_pending = Arc::clone(&pending);
        let worker_jobs = Arc::clone(&jobs);
        let worker_changed = Arc::clone(&changed);
        let worker_stop = Arc::clone(&stop);
        let executor = thread::spawn(move || {
            loop {
                let batch = {
                    let mut queue = worker_jobs
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    while queue.is_empty() && !worker_stop.load(Ordering::SeqCst) {
                        queue = worker_changed
                            .wait(queue)
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                    }
                    if worker_stop.load(Ordering::SeqCst) && queue.is_empty() {
                        break;
                    }
                    std::mem::take(&mut *queue)
                };
                for job in batch {
                    let worker_io = Arc::clone(&io);
                    let job_pending = Arc::clone(&worker_pending);
                    let job_changed = Arc::clone(&worker_changed);
                    thread::spawn(move || {
                        worker_io.release(&job.pack_id, job.tx_id);
                        job_pending.fetch_sub(1, Ordering::SeqCst);
                        job_changed.notify_all();
                    });
                }
            }
        });
        Self {
            jobs,
            changed,
            stop,
            pending,
            executor: Mutex::new(Some(executor)),
        }
    }

    /// Enqueue a release and return immediately on the caller's thread.
    pub fn release(&self, pack_id: &str, tx_id: u64) {
        self.pending.fetch_add(1, Ordering::SeqCst);
        self.lock().push_back(ReleaseJob {
            pack_id: pack_id.to_owned(),
            tx_id,
        });
        self.changed.notify_all();
    }

    /// Diagnostic/test barrier: wait until queued release calls have actually returned.
    /// Never called from the arbiter.
    pub fn wait_idle(&self, timeout: Duration) {
        let started = Instant::now();
        let mut queue = self.lock();
        while self.pending.load(Ordering::SeqCst) > 0 && started.elapsed() < timeout {
            let (guard, _) = self
                .changed
                .wait_timeout(queue, timeout.saturating_sub(started.elapsed()))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            queue = guard;
        }
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<ReleaseJob>> {
        self.jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Drop for ReleaseQueue {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.changed.notify_all();
        if let Some(executor) = self
            .executor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = executor.join();
        }
    }
}
