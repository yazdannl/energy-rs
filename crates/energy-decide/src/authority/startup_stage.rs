//! Startup loading and the single pre-gate stage that appends the diagnostic record.

use crate::authority::startup::record_result;
use crate::authority::startup_evidence as evidence_steps;
use crate::authority::startup_input::StartupInput;
use crate::authority::startup_steps as steps;
use crate::authority::{
    AuditLog, AuthorityAnchor, AuthorityLedger, AuthorityPhase, AuthorityState, RecoveryEvidence,
    StartupResolution,
};

#[derive(Clone)]
pub(crate) struct Stage {
    pub(crate) phase: AuthorityPhase,
    pub(crate) state: AuthorityState,
    pub(crate) reason: Option<&'static str>,
    pub(crate) record_id: Option<u64>,
    pub(crate) warning: String,
    pub(crate) locked_evidence: Option<RecoveryEvidence>,
}

pub(crate) fn load(
    input: &StartupInput<'_>,
) -> (
    Option<AuthorityAnchor>,
    Option<&'static str>,
    AuthorityLedger,
    String,
    bool,
) {
    let (anchor, anchor_error) = match input.anchors.load() {
        Ok(anchor) => (anchor, None),
        Err(error) => (None, Some(error.slug())),
    };
    let bootstrap_allowed = anchor.is_none() && anchor_error.is_none();
    let (ledger, warning, booted) = if bootstrap_allowed {
        steps::bootstrap(
            input.ledger.clone(),
            input.anchors,
            input.now_ms,
            input.run_id,
        )
    } else {
        (input.ledger.clone(), String::new(), false)
    };
    let anchor = if booted {
        input.anchors.load().ok().flatten().or(anchor)
    } else {
        anchor
    };
    (anchor, anchor_error, ledger, warning, booted)
}

pub(crate) fn locked_evidence(
    input: &StartupInput<'_>,
    ledger: &AuthorityLedger,
    anchor: Option<&AuthorityAnchor>,
) -> Option<RecoveryEvidence> {
    if let Some(evidence) = anchor.and_then(|value| value.recovery.clone()) {
        return Some(evidence);
    }
    let (id, _) = evidence_steps::locked_recovery_record(input.resolutions)?;
    (id > 0).then(|| {
        evidence_steps::evidence(
            "recovery_required_unreconciled",
            ledger,
            anchor,
            input.resolutions,
            Some(id),
            input.now_ms,
        )
    })
}

pub(crate) struct StageInput<'a> {
    pub(crate) anchor: Option<&'a AuthorityAnchor>,
    pub(crate) anchor_error: Option<&'static str>,
    pub(crate) ledger: &'a AuthorityLedger,
    pub(crate) warning: String,
    pub(crate) booted: bool,
}

pub(crate) fn stage_for(
    input: &StartupInput<'_>,
    audit: &mut AuditLog,
    stage_input: StageInput<'_>,
) -> Stage {
    let StageInput {
        anchor,
        anchor_error,
        ledger,
        warning,
        booted,
    } = stage_input;
    let phase = steps::phase_for(ledger, booted);
    let state = ledger.latch().map_or(
        AuthorityState::DryRunLatched,
        crate::authority::Latch::state,
    );
    let locked_evidence = if booted {
        None
    } else {
        locked_evidence(input, ledger, anchor)
    };
    let precheck_reason = if locked_evidence.is_some() {
        Some("recovery_required_unreconciled")
    } else {
        anchor_error.or_else(|| {
            steps::precheck(
                ledger,
                if booted { None } else { anchor },
                phase,
                input.resolutions,
                booted,
            )
        })
    };
    let case = if precheck_reason.is_some() {
        StartupResolution::RecoveryRequired
    } else {
        StartupResolution::Reconciled
    };
    let (record_id, failure) = record_result(steps::append_record(
        input, audit, case, phase, anchor, ledger,
    ));
    Stage {
        phase,
        state,
        reason: failure.or(precheck_reason),
        record_id,
        warning,
        locked_evidence,
    }
}
