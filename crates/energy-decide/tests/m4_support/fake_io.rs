//! In-memory pack transport with explicit device application and cancellation.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use energy_decide::dispatch::{Echo, FlowReport, PackCommand, PackIo, WriteAck};

use super::Behavior;

#[derive(Debug, Default)]
struct State {
    behaviors: HashMap<String, Behavior>,
    writes: Vec<(String, PackCommand, u64)>,
    applied: Vec<(String, PackCommand)>,
    active: HashSet<u64>,
    peak_active: usize,
    next_tx: u64,
    released: Vec<u64>,
    begin_at: HashMap<u64, Instant>,
    release_latency_ms: HashMap<u64, u64>,
    reports: BTreeMap<String, Vec<FlowReport>>,
    failures_left: HashMap<String, usize>,
    workers: usize,
    peak_workers: usize,
}

/// In-memory pack transport with observable transactions and device effects.
#[derive(Debug, Default, Clone)]
pub struct FakeIo {
    state: Arc<Mutex<State>>,
}

impl FakeIo {
    /// Build a transport with one behaviour per pack.
    pub fn new(behaviors: &[(&str, Behavior)]) -> Self {
        let mut state = State::default();
        for (pack, behavior) in behaviors {
            state.behaviors.insert((*pack).to_owned(), behavior.clone());
        }
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    /// Queue a report for a later `observe` call.
    pub fn queue(&self, report: FlowReport) {
        self.lock()
            .reports
            .entry(report.pack_id.clone())
            .or_default()
            .push(report);
    }

    /// Transport-entry log: every command that reached the transport.
    pub fn writes(&self) -> Vec<(String, PackCommand, u64)> {
        self.lock().writes.clone()
    }

    /// Device-application log: only commands that physically took effect.
    pub fn applied_commands(&self) -> Vec<PackCommand> {
        self.lock()
            .applied
            .iter()
            .map(|(_, command)| command.clone())
            .collect()
    }

    /// Last wattage physically applied to one pack.
    pub fn device_w(&self, pack_id: &str) -> Option<f64> {
        self.lock()
            .applied
            .iter()
            .rev()
            .find(|(pack, _)| pack == pack_id)
            .map(|(_, command)| command.requested_w)
    }

    /// Number of writes that reached the transport.
    pub fn write_count(&self) -> usize {
        self.lock().writes.len()
    }

    /// Highest number of simultaneously open transactions.
    pub fn peak_active(&self) -> usize {
        self.lock().peak_active
    }

    /// Open transaction ids; empty once the arbiter releases them.
    pub fn active(&self) -> Vec<u64> {
        self.lock().active.iter().copied().collect()
    }

    /// Transaction ids that were explicitly released.
    pub fn released(&self) -> Vec<u64> {
        self.lock().released.clone()
    }

    /// Release latency per transaction, measured from `begin`.
    pub fn release_latency_ms(&self) -> Vec<u64> {
        self.lock().release_latency_ms.values().copied().collect()
    }

    /// Transports currently inside `write`.
    pub fn active_workers(&self) -> usize {
        self.lock().workers
    }

    /// Highest number of simultaneous `write` calls.
    pub fn peak_workers(&self) -> usize {
        self.lock().peak_workers
    }

    /// Replace a behaviour, for retry tests.
    pub fn set_behavior(&self, pack: &str, behavior: Behavior) {
        self.lock().behaviors.insert(pack.to_owned(), behavior);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn apply(&self, pack_id: &str, command: &PackCommand) {
        self.lock()
            .applied
            .push((pack_id.to_owned(), command.clone()));
    }

    fn accepted(&self, pack_id: &str, command: &PackCommand) -> WriteAck {
        self.apply(pack_id, command);
        WriteAck::Accepted {
            echo: Some(Echo {
                limit_w: command.requested_w,
                at_ms: command.issued_at_ms,
            }),
            at_ms: command.issued_at_ms,
        }
    }

    fn wait_cancel(token: &Arc<AtomicBool>, max_wait_ms: u64, at_ms: i64) -> WriteAck {
        let started = Instant::now();
        let limit = Duration::from_millis(max_wait_ms);
        while !token.load(Ordering::SeqCst) && started.elapsed() < limit {
            thread::sleep(Duration::from_millis(1));
        }
        WriteAck::Failed {
            reason: "write_cancelled",
            at_ms,
        }
    }

    fn answer(
        &self,
        behavior: &Behavior,
        pack_id: &str,
        command: &PackCommand,
        deadline_ms: u64,
        token: &Arc<AtomicBool>,
    ) -> WriteAck {
        match behavior {
            Behavior::Accept { echo } => {
                self.apply(pack_id, command);
                WriteAck::Accepted {
                    echo: echo.then_some(Echo {
                        limit_w: command.requested_w,
                        at_ms: command.issued_at_ms,
                    }),
                    at_ms: command.issued_at_ms,
                }
            }
            Behavior::Reject(reason) => WriteAck::Rejected {
                reason,
                at_ms: command.issued_at_ms,
            },
            Behavior::Fail(reason) => WriteAck::Failed {
                reason,
                at_ms: command.issued_at_ms,
            },
            Behavior::Stuck { max_wait_ms } => {
                Self::wait_cancel(token, *max_wait_ms, command.issued_at_ms)
            }
            Behavior::Never => Self::wait_cancel(token, 5_000, command.issued_at_ms),
            Behavior::Late { after_ms } => {
                thread::sleep(Duration::from_millis(deadline_ms.saturating_add(*after_ms)));
                self.accepted(pack_id, command)
            }
            Behavior::FailFirst(count, reason) => self.fail_first(pack_id, command, *count, reason),
        }
    }

    fn fail_first(
        &self,
        pack_id: &str,
        command: &PackCommand,
        count: usize,
        reason: &'static str,
    ) -> WriteAck {
        let mut state = self.lock();
        let left = state
            .failures_left
            .entry(pack_id.to_owned())
            .or_insert(count);
        if *left > 0 {
            *left -= 1;
            WriteAck::Failed {
                reason,
                at_ms: command.issued_at_ms,
            }
        } else {
            drop(state);
            self.accepted(pack_id, command)
        }
    }
}

impl PackIo for FakeIo {
    fn begin(&self, _pack_id: &str) -> u64 {
        let mut state = self.lock();
        state.next_tx += 1;
        let id = state.next_tx;
        state.active.insert(id);
        state.peak_active = state.peak_active.max(state.active.len());
        state.begin_at.insert(id, Instant::now());
        id
    }

    fn release(&self, _pack_id: &str, tx_id: u64) {
        let mut state = self.lock();
        state.active.remove(&tx_id);
        state.released.push(tx_id);
        if let Some(started) = state.begin_at.get(&tx_id) {
            let latency = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            state.release_latency_ms.insert(tx_id, latency);
        }
    }

    fn write(
        &self,
        pack_id: &str,
        command: &PackCommand,
        deadline_ms: u64,
        _tx_id: u64,
        cancel: Arc<AtomicBool>,
    ) -> WriteAck {
        let behavior = self
            .lock()
            .behaviors
            .get(pack_id)
            .cloned()
            .unwrap_or(Behavior::Accept { echo: true });
        {
            let mut state = self.lock();
            state.workers += 1;
            state.peak_workers = state.peak_workers.max(state.workers);
            state
                .writes
                .push((pack_id.to_owned(), command.clone(), deadline_ms));
        }
        let ack = self.answer(&behavior, pack_id, command, deadline_ms, &cancel);
        self.lock().workers -= 1;
        ack
    }

    fn read(&self, pack_id: &str, _deadline_ms: u64) -> Option<FlowReport> {
        self.lock().reports.get_mut(pack_id)?.pop()
    }
}
