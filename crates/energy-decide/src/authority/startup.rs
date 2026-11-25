use crate::authority::{
    AuditEvent, AuditLog, AuthorityLedger, AuthorityPhase, AuthorityRefusal, AuthorityState,
    Caller, EntryPoint, GateOutcome, StartupOutcome, StartupResolution, WriteClass, WriteRequest,
};

use crate::authority::StorageError;
use crate::authority::health;
use crate::authority::startup_input::StartupInput;
use crate::authority::startup_steps as steps;

fn outcome(
    resolution: StartupResolution,
    reason: Option<&'static str>,
    record_id: Option<u64>,
    phase: AuthorityPhase,
    state: AuthorityState,
    warning: String,
) -> StartupOutcome {
    let gate_open =
        phase == AuthorityPhase::Initialized && resolution == StartupResolution::Reconciled;
    StartupOutcome {
        resolution,
        state,
        phase,
        reason,
        record_id,
        gate_open,
        dispatch_eligible: gate_open && state == AuthorityState::Auto,
        warning,
        health: health::report(state, phase, resolution),
    }
}

fn record_result(recorded: Result<u64, StorageError>) -> (Option<u64>, Option<&'static str>) {
    match recorded {
        Ok(id) => (Some(id), None),
        Err(error) => (None, Some(error.slug())),
    }
}

#[derive(Clone)]
struct Stage {
    booted: bool,
    phase: AuthorityPhase,
    state: AuthorityState,
    case: StartupResolution,
    reason: Option<&'static str>,
    record_id: Option<u64>,
    warning: String,
}

/// Execute the exact ordering and return the ledger to use for this process.
pub(crate) fn resolve(
    input: &StartupInput<'_>,
    audit: &mut AuditLog,
) -> (StartupOutcome, crate::authority::AuthorityLedger) {
    let (anchor, anchor_error): (
        Option<crate::authority::AuthorityAnchor>,
        Option<&'static str>,
    ) = match input.anchors.load() {
        Ok(anchor) => (anchor, None),
        Err(error) => (None, Some(error.slug())),
    };
    let (ledger, warning, booted) = if anchor.is_some() {
        (input.ledger.clone(), String::new(), false)
    } else {
        steps::bootstrap(input.ledger.clone(), input.now_ms)
    };
    let phase = steps::phase_for(&ledger, booted);
    let state = ledger.latch().map_or(
        AuthorityState::DryRunLatched,
        crate::authority::Latch::state,
    );
    let precheck_reason = anchor_error
        .or_else(|| steps::precheck(&ledger, anchor.as_ref(), phase, input.resolutions, booted));
    let case = if precheck_reason.is_some() {
        StartupResolution::RecoveryRequired
    } else {
        StartupResolution::Reconciled
    };
    let (record_id, failure) = record_result(steps::append_record(
        input,
        audit,
        case,
        phase,
        anchor.as_ref(),
        &ledger,
    ));
    let stage = Stage {
        booted,
        phase,
        state,
        case,
        reason: failure.or(precheck_reason),
        record_id,
        warning,
    };
    finish(input, audit, anchor.as_ref(), anchor_error, stage, ledger)
}

fn finish(
    input: &StartupInput<'_>,
    audit: &mut AuditLog,
    anchor: Option<&crate::authority::AuthorityAnchor>,
    anchor_error: Option<&'static str>,
    stage: Stage,
    ledger: AuthorityLedger,
) -> (StartupOutcome, AuthorityLedger) {
    if anchor_error.is_none()
        && let (Some(id), None) = (stage.record_id, stage.reason)
        && let Err(error) = steps::install_ref(input.anchors, anchor, &ledger, id, stage.booted)
    {
        let caller = Caller::internal(EntryPoint::AnchorUpdate, input.now_ms, error.slug());
        let request = WriteRequest::new(caller, WriteClass::ControlPlaneAuthority);
        audit.record(AuditEvent {
            seq: 0,
            at_ms: input.now_ms,
            actor: request.caller.actor().to_owned(),
            entry: EntryPoint::AnchorUpdate,
            class: WriteClass::ControlPlaneAuthority,
            state: stage.state,
            phase: stage.phase,
            outcome: GateOutcome::Refused(AuthorityRefusal::AnchorRefPromotionFailed),
            why: request.caller.why().to_owned(),
            detail: error.slug(),
        });
        return (
            outcome(
                StartupResolution::RecoveryRequired,
                Some(error.slug()),
                stage.record_id,
                AuthorityPhase::Invalid,
                AuthorityState::DryRunLatched,
                stage.warning,
            ),
            ledger,
        );
    }
    let effective_phase = if anchor_error.is_some() {
        AuthorityPhase::Invalid
    } else {
        stage.phase
    };
    let final_case = if stage.reason.is_some() {
        StartupResolution::RecoveryRequired
    } else {
        stage.case
    };
    (
        outcome(
            final_case,
            stage.reason,
            stage.record_id,
            effective_phase,
            stage.state,
            stage.warning,
        ),
        ledger,
    )
}
