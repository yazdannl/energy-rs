//! M4 dispatch: one arbiter, one future per pack, and honest outcome rows.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use crate::dispatch::ack_row;
use crate::dispatch::constants::{MAX_DRIVE_POLLS_PER_TICK, NO_FLOW_BACKOFF_MS};
use crate::dispatch::driver::drive;
use crate::dispatch::preparation::{Preparation, prepare};
use crate::dispatch::{
    DispatchContext, DispatchRow, DispatchState, DriveBoundEvent, DriveContext, FlowReport,
    LateWrite, Observer, PackIo, RetryClock, SafetyGate, SlotRequest, WriteAttempt, WriteAuthority,
    WriteFuture,
};

/// Dispatcher with injected consumer ports; it owns no safety or authority decision.
pub struct Dispatcher<S: SafetyGate, A: WriteAuthority, C: RetryClock> {
    io: Arc<dyn PackIo>,
    safety: S,
    authority: A,
    clock: C,
    budget_ms: u64,
    max_polls: u64,
    late: Arc<Mutex<Vec<LateWrite>>>,
    bound: Arc<Mutex<Vec<DriveBoundEvent>>>,
    unresolved: Mutex<Vec<WriteFuture>>,
}
impl<S: SafetyGate, A: WriteAuthority, C: RetryClock> Dispatcher<S, A, C> {
    /// Build a dispatcher with an explicit per-pack budget in milliseconds.
    pub fn new(io: impl PackIo, safety: S, authority: A, clock: C, budget_ms: u64) -> Self {
        Self::new_with_shared(Arc::new(io), safety, authority, clock, budget_ms)
    }

    /// Build a dispatcher over a shared transport so tests can observe it.
    pub fn new_with_shared(
        io: Arc<dyn PackIo>,
        safety: S,
        authority: A,
        clock: C,
        budget_ms: u64,
    ) -> Self {
        Self {
            io,
            safety,
            authority,
            clock,
            budget_ms,
            max_polls: MAX_DRIVE_POLLS_PER_TICK,
            late: Arc::new(Mutex::new(Vec::new())),
            bound: Arc::new(Mutex::new(Vec::new())),
            unresolved: Mutex::new(Vec::new()),
        }
    }

    /// Build a dispatcher with the live M4-owned 20 s per-pack budget.
    pub fn production(io: impl PackIo, safety: S, authority: A, clock: C) -> Self {
        Self::new(
            io,
            safety,
            authority,
            clock,
            crate::dispatch::constants::PACK_IO_BUDGET_MS,
        )
    }

    /// Lower the arbiter poll bound for deterministic loop-bound tests.
    #[must_use]
    pub fn with_max_polls(mut self, max_polls: u64) -> Self {
        self.max_polls = max_polls;
        self
    }

    /// Late completions from superseded generations, drained by the caller.
    pub fn take_late_writes(&self) -> Vec<LateWrite> {
        let mut log = self
            .late
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *log)
    }

    /// Arbiter-loop bound violations, drained by the caller.
    pub fn take_bound_events(&self) -> Vec<DriveBoundEvent> {
        let mut log = self
            .bound
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *log)
    }

    /// Futures still inside transport; never restarted within the same tick.
    pub fn outstanding_futures(&self) -> usize {
        self.unresolved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
    /// Start every pack's future, then drive them all from one arbiter.
    pub fn dispatch_slot(
        &self,
        state: &mut DispatchState,
        requests: &[SlotRequest],
        now_ms: i64,
    ) -> Vec<DispatchRow> {
        let mut preparations: Vec<Preparation> = requests
            .iter()
            .map(|request| {
                prepare(
                    state,
                    request,
                    now_ms,
                    &self.safety,
                    &self.authority,
                    self.budget_ms,
                )
            })
            .collect();
        let mut futures = Vec::new();
        for preparation in &mut preparations {
            if let Preparation::Write(write) = preparation {
                let pack = state.pack(&write.command.pack_id);
                pack.generation = pack.generation.saturating_add(1);
                write.command.generation = pack.generation;
                write.tx_id = self.io.begin(&write.command.pack_id);
                pack.transport_gone = false;
                futures.push(WriteFuture::start(
                    &self.io,
                    &write.command,
                    write.tx_id,
                    self.budget_ms,
                ));
            }
        }
        let driver = DriveContext {
            io: &self.io,
            clock: &self.clock,
            late: &self.late,
            bound: &self.bound,
            max_polls: self.max_polls,
        };
        let result = drive(futures, &driver);
        let outstanding = result
            .unresolved
            .iter()
            .map(|future| future.pack_id().to_owned())
            .collect::<BTreeSet<_>>();
        self.unresolved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(result.unresolved);
        self.collect(state, preparations, &result.attempts, &outstanding, now_ms)
    }

    fn collect(
        &self,
        state: &mut DispatchState,
        preparations: Vec<Preparation>,
        attempts: &[WriteAttempt],
        outstanding: &BTreeSet<String>,
        now_ms: i64,
    ) -> Vec<DispatchRow> {
        let context = DispatchContext {
            io: &self.io,
            clock: &self.clock,
            late: &self.late,
            bound: &self.bound,
            max_polls: self.max_polls,
            budget_ms: self.budget_ms,
            now_ms,
        };
        let mut rows = Vec::with_capacity(preparations.len());
        let mut index = 0;
        for preparation in preparations {
            match preparation {
                Preparation::Row(row) => rows.push(row),
                Preparation::Write(write) => {
                    let attempt = attempts[index].clone();
                    index += 1;
                    let gone = !outstanding.contains(&write.command.pack_id);
                    let mut effect = ack_row::from_attempt(&context, attempt, &write.command);
                    Self::apply_effect(state, &write.command, &mut effect, gone, now_ms);
                    rows.push(crate::dispatch::safety_execution_row::row(
                        effect.row,
                        write.gate_reason,
                        write.interrupted,
                    ));
                }
            }
        }
        rows
    }

    fn apply_effect(
        state: &mut DispatchState,
        command: &crate::dispatch::PackCommand,
        effect: &mut crate::dispatch::AttemptEffect,
        transport_gone: bool,
        now_ms: i64,
    ) {
        let pack = state.pack(&command.pack_id);
        pack.last_command = Some(command.clone());
        pack.pending = effect.pending.take();
        pack.transport_gone = transport_gone;
        if effect.generation_bumped {
            pack.generation = pack.generation.saturating_add(1);
        }
        if let Some(reason) = effect.uncertain_reason {
            pack.uncertain = true;
            pack.uncertain_reason = Some(reason);
            pack.uncertainty_cleared_by = None;
        }
        if effect.arm_backoff {
            pack.backoff_until_ms = now_ms + i64::try_from(NO_FLOW_BACKOFF_MS).unwrap_or(i64::MAX);
            pack.backoff_reason = Some("fault_backoff");
        }
    }

    /// Join futures that have returned; only then is their pack's transport gone.
    pub fn reconcile(&self, state: &mut DispatchState) -> usize {
        let mut unresolved = self
            .unresolved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut gone = BTreeSet::new();
        let finished: Vec<usize> = unresolved
            .iter()
            .enumerate()
            .filter(|(_, future)| future.finished())
            .map(|(index, _)| index)
            .collect();
        for index in finished.into_iter().rev() {
            let future = unresolved.remove(index);
            let deadline = future.deadline();
            let generation = future.generation();
            let pack_id = future.pack_id().to_owned();
            future.release_once(&self.io);
            let result = future.join();
            if result.completed_at > deadline {
                crate::dispatch::driver::record_late(&self.late, &pack_id, generation);
            }
            gone.insert(pack_id);
        }
        for pack_id in state.pack_ids() {
            let pack = state.pack(&pack_id);
            if gone.contains(&pack_id) || !unresolved.iter().any(|f| f.pack_id() == pack_id) {
                pack.transport_gone = true;
            }
        }
        gone.len()
    }

    /// Read one flow report; a read miss keeps tracked command identity and uncertainty.
    pub fn read_flow(
        &self,
        state: &mut DispatchState,
        pack_id: &str,
        now_ms: i64,
        deadline_ms: u64,
    ) -> DispatchRow {
        let Some(report) = self.io.read(pack_id, deadline_ms) else {
            return crate::dispatch::read_miss::row(state, pack_id, now_ms);
        };
        self.observe(state, &report, now_ms)
    }
    /// Re-establish truth only from fresh neutral evidence after the future is gone.
    pub fn read_back(
        &self,
        state: &mut DispatchState,
        report: &FlowReport,
        now_ms: i64,
    ) -> DispatchRow {
        let row = self.observe(state, report, now_ms);
        crate::dispatch::read_back::clear(row, state.pack(&report.pack_id), report, now_ms)
    }
    /// Fold a later report; no waiting occurs in this call.
    pub fn observe(
        &self,
        state: &mut DispatchState,
        report: &FlowReport,
        now_ms: i64,
    ) -> DispatchRow {
        Observer::new(
            &self.io,
            &self.clock,
            &self.late,
            &self.bound,
            self.max_polls,
            self.budget_ms,
        )
        .observe(state, report, now_ms)
    }
}
