//! M4 dispatch: ask both ports, write concurrently under one budget, report honestly.

use std::sync::Arc;

use crate::dispatch::ack_row;
use crate::dispatch::constants::NO_FLOW_BACKOFF_MS;
use crate::dispatch::preparation::{Preparation, prepare};
use crate::dispatch::{
    AppliedReason, DispatchOutcome, DispatchRow, DispatchState, EvidenceKind, FlowReport, Observer,
    PackIo, RetryClock, SafetyGate, SlotRequest, WriteAuthority, WriteTask,
};

/// Dispatcher with injected consumer ports; it owns no safety or authority decision.
pub struct Dispatcher<S: SafetyGate, A: WriteAuthority, C: RetryClock> {
    io: Arc<dyn PackIo>,
    safety: S,
    authority: A,
    clock: C,
    budget_ms: u64,
}

impl<S: SafetyGate, A: WriteAuthority, C: RetryClock> Dispatcher<S, A, C> {
    /// Build a dispatcher with an explicit per-pack budget in milliseconds.
    pub fn new(io: impl PackIo, safety: S, authority: A, clock: C, budget_ms: u64) -> Self {
        Self {
            io: Arc::new(io),
            safety,
            authority,
            clock,
            budget_ms,
        }
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

    /// Start every pack's write before collecting any receipt.
    pub fn dispatch_slot(
        &self,
        state: &mut DispatchState,
        requests: &[SlotRequest],
        now_ms: i64,
    ) -> Vec<DispatchRow> {
        let preparations: Vec<Preparation> = requests
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
        let mut tx_ids = Vec::new();
        for preparation in &preparations {
            if let Preparation::Write(write) = preparation {
                let tx_id = self.io.begin(&write.command.pack_id);
                tx_ids.push(tx_id);
                tasks.push(WriteTask::start(
                    &self.io,
                    &write.command,
                    tx_id,
                    self.budget_ms,
                ));
            }
        }
        self.collect(state, preparations, tasks, &tx_ids, now_ms)
    }

    /// Collect bounded writes and apply their effects to state.
    fn collect(
        &self,
        state: &mut DispatchState,
        preparations: Vec<Preparation>,
        mut tasks: Vec<WriteTask>,
        tx_ids: &[u64],
        now_ms: i64,
    ) -> Vec<DispatchRow> {
        let mut rows = Vec::with_capacity(preparations.len());
        let mut next = 0;
        for preparation in preparations {
            match preparation {
                Preparation::Row(row) => rows.push(row),
                Preparation::Write(write) => {
                    let task = tasks.remove(0);
                    let attempt = task.collect();
                    let tx_id = tx_ids[next];
                    next += 1;
                    self.io.release(&write.command.pack_id, tx_id);
                    let mut effect = ack_row::from_attempt(
                        &self.io,
                        &self.clock,
                        attempt,
                        &write.command,
                        self.budget_ms,
                        now_ms,
                    );
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
        pack.pending = effect.pending.take();
        if let Some(reason) = effect.uncertain_reason {
            pack.uncertain = true;
            pack.uncertain_reason = Some(reason);
        }
        if effect.arm_backoff {
            pack.backoff_until_ms = now_ms + i64::try_from(NO_FLOW_BACKOFF_MS).unwrap_or(i64::MAX);
            pack.backoff_reason = Some("fault_backoff");
        }
    }

    fn gate_row(
        mut row: DispatchRow,
        gate_reason: Option<&'static str>,
        interrupted: bool,
    ) -> DispatchRow {
        if interrupted {
            row = row.reason("pause_interrupted_by_target_change");
        }
        match gate_reason {
            Some(reason) => {
                row.outcome = DispatchOutcome::Skipped;
                row.evidence_kind = EvidenceKind::PortDecision;
                row.reason("safety_preempted").reason(reason)
            }
            None => row,
        }
    }

    /// Read one flow report; a read miss stays null with a typed reason.
    pub fn read_flow(
        &self,
        state: &mut DispatchState,
        pack_id: &str,
        now_ms: i64,
        deadline_ms: u64,
    ) -> DispatchRow {
        let Some(report) = self.io.read(pack_id, deadline_ms) else {
            let command = crate::dispatch::PackCommand {
                pack_id: pack_id.to_owned(),
                requested_w: 0.0,
                issued_at_ms: now_ms,
                fence_id: None,
                safety_traffic: false,
            };
            let mut row =
                DispatchRow::base(&command, crate::dispatch::constants::PACK_IO_BUDGET_MS)
                    .outcome(
                        DispatchOutcome::ConfirmationPending,
                        EvidenceKind::NoMeasurement,
                    )
                    .reason("read_miss");
            row.applied_reason = AppliedReason::ReadMiss;
            return row;
        };
        self.observe(state, &report, now_ms)
    }

    /// Fold a later report; no waiting occurs in this call.
    pub fn observe(
        &self,
        state: &mut DispatchState,
        report: &FlowReport,
        now_ms: i64,
    ) -> DispatchRow {
        Observer::new(&self.io, &self.clock, self.budget_ms).observe(state, report, now_ms)
    }
}
