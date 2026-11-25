use crate::authority::{
    AuditEvent, AuditLog, AuthorityAnchor, AuthorityLedger, AuthorityPhase, AuthorityRefusal,
    AuthorityState, Caller, EntryPoint, GateOutcome, RecoveryEvidence, StartupOutcome,
    StartupResolution, StorageError, WriteClass,
};

use crate::authority::health;
use crate::authority::startup_evidence as evidence_steps;
use crate::authority::startup_input::StartupInput;
use crate::authority::startup_stage::{Stage, StageInput, load, stage_for};
use crate::authority::startup_steps as steps;

struct OutcomeSpec {
    phase: AuthorityPhase,
    state: AuthorityState,
    warning: String,
    evidence: Option<RecoveryEvidence>,
}

fn outcome(
    resolution: StartupResolution,
    reason: Option<&'static str>,
    record_id: Option<u64>,
    spec: OutcomeSpec,
) -> StartupOutcome {
    // Recovery is a verdict plus invariant: effective authority is always latched.
    let state = if resolution == StartupResolution::RecoveryRequired {
        AuthorityState::DryRunLatched
    } else {
        spec.state
    };
    let gate_open =
        spec.phase == AuthorityPhase::Initialized && resolution == StartupResolution::Reconciled;
    StartupOutcome {
        resolution,
        state,
        phase: spec.phase,
        reason,
        evidence: spec.evidence.clone(),
        record_id,
        gate_open,
        dispatch_eligible: gate_open && state == AuthorityState::Auto,
        warning: spec.warning,
        health: health::report(state, spec.phase, resolution, spec.evidence.as_ref()),
    }
}

pub(crate) fn record_result(
    recorded: Result<u64, StorageError>,
) -> (Option<u64>, Option<&'static str>) {
    match recorded {
        Ok(id) => (Some(id), None),
        Err(error) => (None, Some(error.slug())),
    }
}

/// Execute the exact ordering and return the ledger to use for this process.
pub(crate) fn resolve(
    input: &StartupInput<'_>,
    audit: &mut AuditLog,
) -> (StartupOutcome, AuthorityLedger) {
    let (anchor, anchor_error, ledger, warning, booted) = load(input);
    let stage = stage_for(
        input,
        audit,
        StageInput {
            anchor: anchor.as_ref(),
            anchor_error,
            ledger: &ledger,
            warning,
            booted,
        },
    );
    finish(input, audit, anchor.as_ref(), anchor_error, stage, ledger)
}

fn build_evidence(
    input: &StartupInput<'_>,
    anchor: Option<&AuthorityAnchor>,
    stage: &Stage,
    ledger: &AuthorityLedger,
) -> RecoveryEvidence {
    let reason = stage.reason.unwrap_or("recovery_required_unreconciled");
    let mut value = evidence_steps::evidence(
        reason,
        ledger,
        anchor,
        input.resolutions,
        stage.record_id,
        input.now_ms,
    );
    if let Some(locked) = &stage.locked_evidence {
        value.reason.clone_from(&locked.reason);
        value.since_ms = locked.since_ms;
    }
    value
}

fn finish_recovery(
    input: &StartupInput<'_>,
    audit: &mut AuditLog,
    anchor: Option<&AuthorityAnchor>,
    stage: Stage,
    mut ledger: AuthorityLedger,
    evidence: RecoveryEvidence,
) -> (StartupOutcome, AuthorityLedger) {
    ledger.append(
        input.now_ms,
        "authority",
        "recovery_required",
        crate::authority::AuthorityEventKind::RecoveryRequired {
            reason: evidence.reason.clone(),
            evidence: evidence.detail(),
            record_id: stage.record_id.unwrap_or(0),
        },
    );
    let caller = Caller::internal(EntryPoint::Startup, input.now_ms, "startup reconciliation");
    audit.record(AuditEvent {
        seq: 0,
        at_ms: input.now_ms,
        actor: caller.actor().to_owned(),
        entry: EntryPoint::Startup,
        class: WriteClass::ControlPlaneAuthority,
        state: AuthorityState::DryRunLatched,
        phase: stage.phase,
        outcome: GateOutcome::Refused(AuthorityRefusal::AuthorityRecoveryRequired),
        why: caller.why().to_owned(),
        detail: evidence.detail(),
        evidence: Some(evidence.clone()),
    });
    if evidence_steps::persist_recovery(input.anchors, anchor, &evidence).is_err() {
        return (
            outcome(
                StartupResolution::RecoveryRequired,
                Some("recovery_record_corrupt"),
                stage.record_id,
                OutcomeSpec {
                    phase: AuthorityPhase::Invalid,
                    state: AuthorityState::DryRunLatched,
                    warning: stage.warning,
                    evidence: None,
                },
            ),
            ledger,
        );
    }
    (
        outcome(
            StartupResolution::RecoveryRequired,
            stage.reason,
            stage.record_id,
            OutcomeSpec {
                phase: stage.phase,
                state: stage.state,
                warning: stage.warning,
                evidence: Some(evidence),
            },
        ),
        ledger,
    )
}

fn install_failure(
    input: &StartupInput<'_>,
    audit: &mut AuditLog,
    stage: &Stage,
    error: StorageError,
) {
    let caller = Caller::internal(EntryPoint::AnchorUpdate, input.now_ms, error.slug());
    audit.record(AuditEvent {
        seq: 0,
        at_ms: input.now_ms,
        actor: caller.actor().to_owned(),
        entry: EntryPoint::AnchorUpdate,
        class: WriteClass::ControlPlaneAuthority,
        state: stage.state,
        phase: stage.phase,
        outcome: GateOutcome::Refused(AuthorityRefusal::AnchorRefPromotionFailed),
        why: caller.why().to_owned(),
        detail: error.slug().to_owned(),
        evidence: None,
    });
}

fn finish_reconciled(
    input: &StartupInput<'_>,
    audit: &mut AuditLog,
    anchor: Option<&AuthorityAnchor>,
    anchor_error: Option<&'static str>,
    stage: Stage,
    ledger: AuthorityLedger,
) -> (StartupOutcome, AuthorityLedger) {
    if anchor_error.is_none()
        && let Some(id) = stage.record_id
        && let Err(error) = steps::install_ref(input.anchors, anchor, &ledger, id, false)
    {
        let reason = error.slug();
        install_failure(input, audit, &stage, error);
        let evidence = evidence_steps::evidence(
            reason,
            &ledger,
            anchor,
            input.resolutions,
            stage.record_id,
            input.now_ms,
        );
        let mut failed_stage = stage;
        failed_stage.reason = Some(reason);
        return finish_recovery(input, audit, anchor, failed_stage, ledger, evidence);
    }
    let phase = if anchor_error.is_some() {
        AuthorityPhase::Invalid
    } else {
        stage.phase
    };
    (
        outcome(
            StartupResolution::Reconciled,
            None,
            stage.record_id,
            OutcomeSpec {
                phase,
                state: stage.state,
                warning: stage.warning,
                evidence: None,
            },
        ),
        ledger,
    )
}

fn finish(
    input: &StartupInput<'_>,
    audit: &mut AuditLog,
    anchor: Option<&AuthorityAnchor>,
    anchor_error: Option<&'static str>,
    stage: Stage,
    ledger: AuthorityLedger,
) -> (StartupOutcome, AuthorityLedger) {
    if stage.reason.is_some() {
        let evidence = build_evidence(input, anchor, &stage, &ledger);
        return finish_recovery(input, audit, anchor, stage, ledger, evidence);
    }
    finish_reconciled(input, audit, anchor, anchor_error, stage, ledger)
}
