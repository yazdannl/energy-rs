//! Turn arbiter attempts into honest rows without growing the dispatcher.

use std::collections::BTreeSet;

use crate::dispatch::{
    AttemptEffect, DispatchContext, DispatchRow, DispatchState, Preparation, WriteAttempt,
};

/// Fold preparations and arbiter attempts into one row per request.
pub fn collect(
    state: &mut DispatchState,
    preparations: Vec<Preparation>,
    attempts: &[WriteAttempt],
    outstanding: &BTreeSet<String>,
    now_ms: i64,
    context: &DispatchContext<'_>,
) -> Vec<DispatchRow> {
    let mut rows = Vec::with_capacity(preparations.len());
    let mut index = 0;
    for preparation in preparations {
        match preparation {
            Preparation::Row(row) => rows.push(row),
            Preparation::Write(write) => {
                let attempt = attempts[index].clone();
                index += 1;
                let gone = !outstanding.contains(&write.command.pack_id);
                let mut effect =
                    crate::dispatch::ack_row::from_attempt(context, attempt, &write.command);
                apply_effect(state, &write.command, &mut effect, gone, now_ms);
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
    effect: &mut AttemptEffect,
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
        pack.backoff_until_ms = now_ms
            + i64::try_from(crate::dispatch::constants::NO_FLOW_BACKOFF_MS).unwrap_or(i64::MAX);
        pack.backoff_reason = Some("fault_backoff");
    }
}
