//! M4 dispatch: ask both ports, write concurrently under one budget, report honestly.

use std::sync::{Arc, Mutex};

use crate::dispatch::ack_row;
use crate::dispatch::constants::NO_FLOW_BACKOFF_MS;
use crate::dispatch::preparation::{Preparation, prepare};
use crate::dispatch::{
    DispatchContext, DispatchOutcome, DispatchRow, DispatchState, EvidenceKind, FlowReport,
    LateWrite, Observer, PackIo, RetryClock, SafetyGate, SlotRequest, WorkerHandle,
    WorkerTerminationViolation, WriteAuthority, WriteTask, WriteTaskContext, status_direction,
};

/// Dispatcher with injected consumer ports; it owns no safety or authority decision.
pub struct Dispatcher<S: SafetyGate, A: WriteAuthority, C: RetryClock> {
    io: Arc<dyn PackIo>,
    safety: S,
    authority: A,
    clock: C,
    budget_ms: u64,
    late: Arc<Mutex<Vec<LateWrite>>>,
    violations: Arc<Mutex<Vec<WorkerTerminationViolation>>>,
    workers: Arc<Mutex<Vec<WorkerHandle>>>,
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
            late: Arc::new(Mutex::new(Vec::new())),
            violations: Arc::new(Mutex::new(Vec::new())),
            workers: Arc::new(Mutex::new(Vec::new())),
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

    /// Late completions from superseded write generations, drained by the caller.
    pub fn take_late_writes(&self) -> Vec<LateWrite> {
        let mut log = self
            .late
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *log)
    }

    /// Worker bound violations, drained by the caller.
    pub fn take_worker_violations(&self) -> Vec<WorkerTerminationViolation> {
        let mut log = self
            .violations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.prune_workers();
        std::mem::take(&mut *log)
    }

    fn prune_workers(&self) {
        self.workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|worker| !worker.done.load(std::sync::atomic::Ordering::SeqCst));
    }

    /// Workers still inside a transport call; bounded by the termination bound.
    pub fn outstanding_workers(&self) -> usize {
        self.prune_workers();
        self.workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|worker| !worker.done.load(std::sync::atomic::Ordering::SeqCst))
            .count()
    }

    /// Start every pack's write before collecting any receipt.
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
        let mut tasks = Vec::new();
        let task_context = WriteTaskContext {
            late: &self.late,
            violations: &self.violations,
            workers: &self.workers,
        };
        for preparation in &mut preparations {
            if let Preparation::Write(write) = preparation {
                let pack = state.pack(&write.command.pack_id);
                pack.generation = pack.generation.saturating_add(1);
                write.command.generation = pack.generation;
                write.tx_id = self.io.begin(&write.command.pack_id);
                tasks.push(WriteTask::start(
                    &self.io,
                    &write.command,
                    write.tx_id,
                    self.budget_ms,
                    pack.generation,
                    &task_context,
                ));
            }
        }
        self.collect(state, preparations, tasks, now_ms)
    }

    /// Collect bounded writes and apply their effects to state.
    fn collect(
        &self,
        state: &mut DispatchState,
        preparations: Vec<Preparation>,
        mut tasks: Vec<WriteTask>,
        now_ms: i64,
    ) -> Vec<DispatchRow> {
        let mut rows = Vec::with_capacity(preparations.len());
        for preparation in preparations {
            match preparation {
                Preparation::Row(row) => rows.push(row),
                Preparation::Write(write) => {
                    let attempt = tasks.remove(0).collect();
                    let context = DispatchContext {
                        io: &self.io,
                        clock: &self.clock,
                        late: &self.late,
                        violations: &self.violations,
                        workers: &self.workers,
                        budget_ms: self.budget_ms,
                        now_ms,
                    };
                    let mut effect = ack_row::from_attempt(&context, attempt, &write.command);
                    Self::apply_effect(state, &write.command, &mut effect, now_ms);
                    rows.push(Self::gate_row(
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
        now_ms: i64,
    ) {
        let pack = state.pack(&command.pack_id);
        pack.last_command = Some(command.clone());
        pack.pending = effect.pending.take();
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

    /// Annotate economic preemption without hiding the safety command's own outcome.
    fn gate_row(
        mut row: DispatchRow,
        gate_reason: Option<&'static str>,
        interrupted: bool,
    ) -> DispatchRow {
        if interrupted {
            row = row.reason("pause_interrupted_by_target_change");
        }
        if row.safety_traffic {
            let (execution, evidence, detail) =
                crate::dispatch::safety_execution_row::execution(&row);
            row.safety_execution = Some(execution);
            row.safety_evidence = Some(evidence);
            row.safety_reason = detail;
        }
        if let Some(reason) = gate_reason {
            row.economic_preempted_by = Some(reason);
            row = row.reason("safety_preempted").reason(reason);
        }
        row
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

    /// Re-establish truth from a device read-back with an echo and idle flow.
    ///
    /// Once a request has left for the device it cannot be un-sent. A read-back that proves
    /// the device is neutral is one of the two ways to clear uncertainty; the other is a
    /// verified hold. The clearing method is recorded in the row and pack state.
    pub fn read_back(
        &self,
        state: &mut DispatchState,
        report: &FlowReport,
        now_ms: i64,
    ) -> DispatchRow {
        let mut row = self.observe(state, report, now_ms);
        let idle = status_direction(report.ac_status, report.dc_status)
            == crate::dispatch::FlowStatus::Idle
            && report.measured_w.abs() <= crate::dispatch::constants::HOLD_DEADBAND_W;
        let pack = state.pack(&report.pack_id);
        if pack.uncertain && report.echo.is_some() && idle {
            pack.uncertain = false;
            pack.uncertain_reason = None;
            pack.uncertainty_cleared_by = Some("read_back");
            row = row
                .outcome(
                    DispatchOutcome::Confirmed,
                    EvidenceKind::CommandVerification,
                )
                .reason("read_back_verified");
            row.uncertainty_cleared_by = Some("read_back");
        }
        row
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
            &self.violations,
            &self.workers,
            self.budget_ms,
        )
        .observe(state, report, now_ms)
    }
}
