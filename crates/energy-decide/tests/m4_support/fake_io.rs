//! In-memory pack transport with observable transactions.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::thread;

use energy_decide::dispatch::{Echo, FlowReport, PackCommand, PackIo, WriteAck};

use super::Behavior;

#[derive(Debug, Default)]
struct State {
    behaviors: HashMap<String, Behavior>,
    writes: Vec<(String, PackCommand, u64)>,
    active: HashSet<u64>,
    peak_active: usize,
    next_tx: u64,
    released: Vec<u64>,
    reports: BTreeMap<String, Vec<FlowReport>>,
    failures_left: HashMap<String, usize>,
}

/// In-memory pack transport with observable transaction lifetime.
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
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .reports
            .entry(report.pack_id.clone())
            .or_default()
            .push(report);
    }

    /// Commands that reached the fake transport, in order.
    pub fn writes(&self) -> Vec<(String, PackCommand, u64)> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .writes
            .clone()
    }

    /// Number of writes that reached the fake transport.
    pub fn write_count(&self) -> usize {
        self.writes().len()
    }

    /// Highest number of simultaneously open transactions.
    pub fn peak_active(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .peak_active
    }

    /// Open transaction ids; empty once the coordinator releases them.
    pub fn active(&self) -> Vec<u64> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active.iter().copied().collect()
    }

    /// Transaction ids that were explicitly released.
    pub fn released(&self) -> Vec<u64> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .released
            .clone()
    }

    /// Replace a behaviour, for retry tests.
    pub fn set_behavior(&self, pack: &str, behavior: Behavior) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.behaviors.insert(pack.to_owned(), behavior);
    }
    fn answer(&self, behavior: &Behavior, pack_id: &str, command: &PackCommand) -> WriteAck {
        let accepted = || WriteAck::Accepted {
            echo: Some(Echo {
                limit_w: command.requested_w,
                at_ms: command.issued_at_ms,
            }),
            at_ms: command.issued_at_ms,
        };
        match behavior {
            Behavior::Accept { echo } => WriteAck::Accepted {
                echo: echo.then_some(Echo {
                    limit_w: command.requested_w,
                    at_ms: command.issued_at_ms,
                }),
                at_ms: command.issued_at_ms,
            },
            Behavior::Reject(reason) => WriteAck::Rejected {
                reason,
                at_ms: command.issued_at_ms,
            },
            Behavior::Fail(reason) => WriteAck::Failed {
                reason,
                at_ms: command.issued_at_ms,
            },
            Behavior::Never => loop {
                thread::park();
            },
            Behavior::FailFirst(count, reason) => {
                self.fail_first(pack_id, command, *count, reason, accepted())
            }
        }
    }

    fn fail_first(
        &self,
        pack_id: &str,
        command: &PackCommand,
        count: usize,
        reason: &'static str,
        accepted: WriteAck,
    ) -> WriteAck {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
            accepted
        }
    }
}

impl PackIo for FakeIo {
    fn begin(&self, _pack_id: &str) -> u64 {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.next_tx += 1;
        let id = state.next_tx;
        state.active.insert(id);
        state.peak_active = state.peak_active.max(state.active.len());
        id
    }

    fn release(&self, _pack_id: &str, tx_id: u64) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active.remove(&tx_id);
        state.released.push(tx_id);
    }

    fn write(&self, pack_id: &str, command: &PackCommand, deadline_ms: u64) -> WriteAck {
        let behavior = {
            let state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state
                .behaviors
                .get(pack_id)
                .cloned()
                .unwrap_or(Behavior::Accept { echo: true })
        };
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .writes
            .push((pack_id.to_owned(), command.clone(), deadline_ms));
        self.answer(&behavior, pack_id, command)
    }

    fn read(&self, pack_id: &str, _deadline_ms: u64) -> Option<FlowReport> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.reports.get_mut(pack_id)?.pop()
    }
}
