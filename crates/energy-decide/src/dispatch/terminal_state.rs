//! Synchronized terminal-state decision shared by a write future and the arbiter.
//!
//! A std-only crate cannot make an `Instant` atomic, so the recorded completion is
//! published through a mutex. An atomic early-completion flag closes the gap between
//! the transport returning and that mutex being acquired. The arbiter never infers
//! completion from `JoinHandle::is_finished`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::dispatch::{WriteResult, WriteTerminal};

/// One future's terminal decision, published exactly once.
pub struct TerminalState {
    terminal: Mutex<WriteTerminal>,
    changed: Condvar,
    completed_before_deadline: AtomicBool,
}

impl Default for TerminalState {
    fn default() -> Self {
        Self {
            terminal: Mutex::new(WriteTerminal::Pending),
            changed: Condvar::new(),
            completed_before_deadline: AtomicBool::new(false),
        }
    }
}

impl TerminalState {
    /// Publish the transport return value; the first publication wins.
    pub fn publish(&self, result: WriteResult, deadline: Instant) {
        if result.completed_at <= deadline {
            self.completed_before_deadline.store(true, Ordering::SeqCst);
        }
        let mut terminal = self.lock();
        if matches!(*terminal, WriteTerminal::Pending) {
            *terminal = WriteTerminal::Completed(result);
        }
        self.changed.notify_all();
    }

    /// Decide Completed or Expired once, from recorded time, never from handle state.
    pub fn settle(&self, now: Instant, deadline: Instant) -> Option<WriteTerminal> {
        let mut terminal = self.lock();
        loop {
            match &*terminal {
                WriteTerminal::Completed(result) => {
                    return Some(WriteTerminal::Completed(result.clone()));
                }
                WriteTerminal::Expired => return Some(WriteTerminal::Expired),
                WriteTerminal::Pending => {}
            }
            if now < deadline {
                return None;
            }
            if self.completed_before_deadline.load(Ordering::SeqCst) {
                let (guard, wait) = self
                    .changed
                    .wait_timeout(terminal, Duration::from_millis(5))
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                terminal = guard;
                if wait.timed_out() {
                    *terminal = WriteTerminal::Expired;
                    return Some(WriteTerminal::Expired);
                }
                continue;
            }
            *terminal = WriteTerminal::Expired;
            return Some(WriteTerminal::Expired);
        }
    }

    fn lock(&self) -> MutexGuard<'_, WriteTerminal> {
        self.terminal
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
