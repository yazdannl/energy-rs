use crate::authority::{
    AnchorStore, AuditEvent, AuditLog, AuthorityAnchor, AuthorityLedger, AuthorityPhase,
    AuthorityState, BootstrapAuthorityTx, Caller, EntryPoint, GateOutcome, ResolutionStore,
    StartupResolution, StartupResolutionRecord, StorageError, WriteClass, WriteRequest, gate,
    journal, startup_input::StartupInput,
};

enum BootstrapAttempt {
    Booted {
        ledger: AuthorityLedger,
        warning: String,
    },
    NotAllowed(AuthorityLedger),
    Failed(AuthorityLedger),
}

fn try_bootstrap(source: AuthorityLedger, now_ms: i64) -> BootstrapAttempt {
    let Ok(mut tx) = BootstrapAuthorityTx::begin(source.clone(), now_ms, "authority") else {
        return BootstrapAttempt::NotAllowed(source);
    };
    let token = tx.token();
    let event = tx.event();
    if tx.write_token(token, 1).is_err() || tx.write_event(event).is_err() {
        return BootstrapAttempt::Failed(source);
    }
    match tx.commit() {
        Ok(committed) => {
            let warning = committed.warning().to_owned();
            BootstrapAttempt::Booted {
                ledger: committed.into_ledger(),
                warning,
            }
        }
        Err(_) => BootstrapAttempt::Failed(source),
    }
}

pub(crate) fn bootstrap(source: AuthorityLedger, now_ms: i64) -> (AuthorityLedger, String, bool) {
    match try_bootstrap(source, now_ms) {
        BootstrapAttempt::Booted { ledger, warning } => (ledger, warning, true),
        BootstrapAttempt::NotAllowed(ledger) | BootstrapAttempt::Failed(ledger) => {
            (ledger, String::new(), false)
        }
    }
}

pub(crate) fn phase_for(ledger: &AuthorityLedger, booted: bool) -> AuthorityPhase {
    if booted {
        return AuthorityPhase::Initialized;
    }
    let Some(latch) = ledger.latch() else {
        return AuthorityPhase::Invalid;
    };
    let valid = latch.token().generation().is_some()
        && ledger.explains_generation(latch.token().generation());
    if valid {
        AuthorityPhase::Initialized
    } else {
        AuthorityPhase::Invalid
    }
}

fn heads_reconcile(ledger: &AuthorityLedger, anchor: &AuthorityAnchor) -> bool {
    let head = ledger.head_event_id();
    if anchor.committed_head > head {
        return false;
    }
    if head == anchor.committed_head {
        return true;
    }
    head == anchor.pending_head && anchor.pending_head > anchor.committed_head
}

pub(crate) fn precheck(
    ledger: &AuthorityLedger,
    anchor: Option<&AuthorityAnchor>,
    phase: AuthorityPhase,
    resolutions: &dyn ResolutionStore,
    booted: bool,
) -> Option<&'static str> {
    if phase != AuthorityPhase::Initialized {
        return Some("latch_token_invalid");
    }
    let latch = ledger.latch()?;
    let generation = latch.token().generation()?;
    if !ledger.journal_complete() {
        return Some("journal_incomplete");
    }
    let Some(anchor) = anchor else {
        return if booted {
            None
        } else {
            Some("anchor_mismatch")
        };
    };
    if anchor.generation != generation || anchor.token_hash != latch.token().hash() {
        return Some("anchor_mismatch");
    }
    if anchor.checkpoint_event_id != ledger.checkpoint_event_id()
        || anchor.journal_hash_chain_head != ledger.chain_head()
    {
        return Some("anchor_mismatch");
    }
    if !heads_reconcile(ledger, anchor) {
        return Some("anchor_mismatch");
    }
    if let Some(id) = anchor.last_startup_resolution_ref
        && !matches!(resolutions.read(id), Ok(Some(_)))
    {
        return Some("anchor_mismatch");
    }
    None
}

fn hashes(anchor: Option<&AuthorityAnchor>, ledger: &AuthorityLedger) -> (String, String) {
    let anchor_hash = anchor.map_or_else(
        || journal::hash_text("missing-anchor"),
        |value| journal::hash_text(&format!("{value:?}")),
    );
    let checkpoint_hash = journal::hash_text(&ledger.checkpoint_event_id().to_string());
    (anchor_hash, checkpoint_hash)
}

pub(crate) fn append_record(
    input: &StartupInput<'_>,
    audit: &mut AuditLog,
    case: StartupResolution,
    phase: AuthorityPhase,
    anchor: Option<&AuthorityAnchor>,
    ledger: &AuthorityLedger,
) -> Result<u64, StorageError> {
    let state = ledger.latch().map_or(
        AuthorityState::DryRunLatched,
        crate::authority::Latch::state,
    );
    let caller = Caller::internal(EntryPoint::StartupResolutionWrite, input.now_ms, "startup");
    let request = WriteRequest::new(caller, WriteClass::StartupResolution);
    let outcome = gate::decide(
        &request,
        phase,
        state,
        StartupResolution::Reconciled,
        &std::collections::HashSet::new(),
    );
    audit.record(AuditEvent {
        seq: 0,
        at_ms: input.now_ms,
        actor: request.caller.actor().to_owned(),
        entry: EntryPoint::StartupResolutionWrite,
        class: WriteClass::StartupResolution,
        state,
        phase,
        outcome: outcome.clone(),
        why: request.caller.why().to_owned(),
        detail: "startup_resolution",
    });
    if outcome != GateOutcome::Authorized {
        return Err(StorageError::Corrupt);
    }
    let id = input.resolutions.next_id()?;
    let (anchor_hash, checkpoint_hash) = hashes(anchor, ledger);
    let record = StartupResolutionRecord {
        id,
        case,
        at: input.now_ms,
        anchor_hash,
        checkpoint_hash,
        journal_head_ref: ledger.head_event_id(),
    };
    input.resolutions.append(&record)
}

pub(crate) fn install_ref(
    anchors: &dyn AnchorStore,
    anchor: Option<&AuthorityAnchor>,
    ledger: &AuthorityLedger,
    record_id: u64,
    first_bootstrap: bool,
) -> Result<(), StorageError> {
    if first_bootstrap {
        let token_hash = ledger
            .latch()
            .map_or_else(String::new, |latch| latch.token().hash().to_owned());
        let created = AuthorityAnchor::new(
            ledger
                .latch()
                .map_or(0, |latch| latch.token().generation().unwrap_or(0)),
            &token_hash,
            ledger.head_event_id(),
            ledger.chain_head(),
            ledger.checkpoint_event_id(),
            record_id,
        );
        return anchors.atomic_write(&created);
    }
    let Some(existing) = anchor else {
        return Err(StorageError::PendingRefFailed);
    };
    let mut prepared = existing.clone();
    if prepared.pending_head == ledger.head_event_id()
        && prepared.pending_head > prepared.committed_head
    {
        prepared.adopt_pending(ledger.head_event_id());
    } else if prepared.pending_head > prepared.committed_head {
        prepared.discard_pending();
    }
    prepared.prepare_ref(record_id);
    anchors.atomic_write(&prepared)?;
    if !prepared.promote_ref() {
        return Err(StorageError::PromotionFailed);
    }
    anchors.atomic_write(&prepared)
}
