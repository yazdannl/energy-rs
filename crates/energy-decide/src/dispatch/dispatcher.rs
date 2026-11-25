//! M4 dispatch: one arbiter, one future per pack, and honest outcome rows.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use crate::authority::ReconcilePermit;
use crate::dispatch::constants::{MAX_DRIVE_POLLS_PER_TICK, TRANSPORT_BOUND_MS};
use crate::dispatch::driver::drive;
use crate::dispatch::preparation::{Preparation, prepare};
use crate::dispatch::{
    DispatchContext, DispatchRow, DispatchState, DriveBoundEvent, DriveContext, FlowReport,
    LateWrite, Observer, PackIo, RetryClock, SafetyGate, SlotRequest, TransactionGuard,
    WriteAuthority, WriteFuture,
};

/// Dispatcher with injected consumer ports; it owns no safety or authority decision.
pub struct Dispatcher<S: SafetyGate, A: WriteAuthority, C: RetryClock> {
    io: Arc<dyn PackIo>,
    safety: S,
    authority: A,
    clock: C,
    budget_ms: u64,
    transport_bound_ms: u64,
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
            transport_bound_ms: TRANSPORT_BOUND_MS,
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

    /// Lower the hard transport bound for deterministic contract-violation tests.
    /// Production always uses `TRANSPORT_BOUND_MS` from the live engine contract.
    #[must_use]
    pub fn with_transport_bound_ms(mut self, transport_bound_ms: u64) -> Self {
        self.transport_bound_ms = transport_bound_ms;
        self
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

    /// Unresolved futures still awaiting reaping; the observable reconcile condition.
    pub fn outstanding_futures(&self) -> usize {
        self.unresolved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
    fn contexts(&self, now_ms: i64) -> (DispatchContext<'_>, DriveContext<'_>) {
        let context = DispatchContext {
            io: &self.io,
            clock: &self.clock,
            late: &self.late,
            bound: &self.bound,
            max_polls: self.max_polls,
            budget_ms: self.budget_ms,
            transport_bound_ms: self.transport_bound_ms,
            now_ms,
        };
        let driver = DriveContext {
            io: context.io,
            clock: context.clock,
            late: context.late,
            bound: context.bound,
            transport_bound_ms: context.transport_bound_ms,
            max_polls: context.max_polls,
        };
        (context, driver)
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
                let guard = TransactionGuard::new(
                    Arc::clone(&self.io),
                    &write.command.pack_id,
                    write.tx_id,
                );
                pack.transport_gone = false;
                pack.last_apply_attempt_ms = now_ms;
                pack.ordered_application = Some(self.io.ordered_application());
                futures.push(WriteFuture::start(
                    &self.io,
                    &write.command,
                    guard,
                    self.budget_ms,
                    self.transport_bound_ms,
                ));
            }
        }
        let (context, driver) = self.contexts(now_ms);
        let result = drive(futures, &driver);
        let outstanding = result
            .unresolved
            .iter()
            .map(|future| future.pack_id().to_owned())
            .collect::<BTreeSet<_>>();
        let mut pending = self
            .unresolved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending.extend(result.unresolved);
        pending.extend(result.retired);
        crate::dispatch::slot_collector::collect(
            state,
            preparations,
            &result.attempts,
            &outstanding,
            now_ms,
            &context,
        )
    }

    /// Join finished futures only with an unforgeable authority reconcile permit.
    pub fn reconcile(
        &self,
        state: &mut DispatchState,
        now_ms: i64,
        _permit: &ReconcilePermit,
    ) -> usize {
        let mut unresolved = self
            .unresolved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut gone = BTreeSet::new();
        let reapable: Vec<usize> = unresolved
            .iter()
            .enumerate()
            .filter(|(_, future)| future.handle_finished())
            .map(|(index, _)| index)
            .collect();
        for index in reapable.into_iter().rev() {
            let future = unresolved.remove(index);
            let pack_id = future.pack_id().to_owned();
            let deadline = future.deadline();
            let generation = future.generation();
            let claim_late = future.claim_late_log();
            let result = future.reap_finished();
            if result.completed_at > deadline && claim_late {
                crate::dispatch::driver::record_late(&self.late, &pack_id, generation);
            }
            let pack = state.pack(&pack_id);
            pack.transport_contract_violated = false;
            pack.joined_at_ms = now_ms;
        }
        for pack_id in state.pack_ids() {
            if unresolved.iter().any(|f| f.pack_id() == pack_id) {
                state.pack(&pack_id).transport_gone = false;
                continue;
            }
            let pack = state.pack(&pack_id);
            if !pack.transport_gone {
                pack.joined_at_ms = now_ms;
            }
            pack.transport_gone = true;
            gone.insert(pack_id);
            if pack.uncertain && pack.ordered_application != Some(true) {
                pack.uncertain = false;
                pack.uncertain_reason = None;
                pack.uncertainty_cleared_by = Some("explicit_reconcile");
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
        let empty = crate::dispatch::PackState::default();
        let tracked = state.get(&report.pack_id).unwrap_or(&empty);
        let row = crate::dispatch::read_back::row(tracked, report, self.budget_ms);
        crate::dispatch::read_back::clear(row, state.pack(&report.pack_id), report, now_ms)
    }
    /// Fold a later report; no waiting occurs in this call.
    pub fn observe(
        &self,
        state: &mut DispatchState,
        report: &FlowReport,
        now_ms: i64,
    ) -> DispatchRow {
        let escalation = crate::dispatch::observer_escalation::Escalation {
            io: &self.io,
            clock: &self.clock,
            late: &self.late,
            bound: &self.bound,
            transport_bound_ms: self.transport_bound_ms,
            max_polls: self.max_polls,
            budget_ms: self.budget_ms,
        };
        Observer::new(&escalation).observe(state, report, now_ms)
    }
}
