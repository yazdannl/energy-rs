use crate::authority::{
    AnchorStore, AuditEvent, AuditLog, AuthorityAnchor, AuthorityLedger, AuthorityPhase,
    AuthorityState, BootstrapAuthorityTx, BootstrapIssue, Caller, EntryPoint, GateOutcome,
    IssuedBootstrapCapability, ResolutionStore, StartupResolution, StartupResolutionRecord,
    StorageError, WriteClass, WriteRequest, gate, journal, startup_input::StartupInput,
};

enum BootstrapAttempt {
    Booted {
        ledger: AuthorityLedger,
        warning: String,
    },
    NotAllowed(AuthorityLedger),
    Failed(AuthorityLedger),
}

fn try_bootstrap(
    source: AuthorityLedger,
    anchors: &dyn crate::authority::AnchorStore,
    now_ms: i64,
    run_id: u64,
) -> BootstrapAttempt {
    let tx_id = run_id.wrapping_add(1);
    let nonce = journal::hash_bytes(run_id, source.seed().as_bytes());
    let mut anchor = match anchors.load() {
        Ok(Some(existing)) => existing,
        Ok(None) => {
            let created = AuthorityAnchor::new(0, "", 0, 0, 0, 0);
            if anchors.atomic_write(&created).is_err() {
                return BootstrapAttempt::Failed(source);
            }
            created
        }
        Err(_) => return BootstrapAttempt::Failed(source),
    };
    let issue = if let Some(issue) = anchor.bootstrap_issue.take() {
        issue
    } else {
        let issue = BootstrapIssue::new(tx_id, run_id, nonce, now_ms);
        anchor.issue_bootstrap(issue.clone());
        if anchors.atomic_write(&anchor).is_err() {
            return BootstrapAttempt::Failed(source);
        }
        issue
    };
    if issue.consumed || issue.run_id != run_id || issue.tx_id != tx_id || issue.nonce != nonce {
        return BootstrapAttempt::NotAllowed(source);
    }
    let capability = IssuedBootstrapCapability::new(run_id, tx_id, nonce);
    let Ok(mut tx) =
        BootstrapAuthorityTx::begin(source.clone(), now_ms, "authority", anchors, &capability)
    else {
        return BootstrapAttempt::NotAllowed(source);
    };
    // Fail closed unless `begin` durably consumed the marker before returning.
    if !anchors
        .load()
        .ok()
        .flatten()
        .and_then(|stored| stored.bootstrap_issue)
        .is_some_and(|issue| issue.consumed)
    {
        return BootstrapAttempt::Failed(source);
    }
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

pub(crate) fn bootstrap(
    source: AuthorityLedger,
    anchors: &dyn crate::authority::AnchorStore,
    now_ms: i64,
    run_id: u64,
) -> (AuthorityLedger, String, bool) {
    match try_bootstrap(source, anchors, now_ms, run_id) {
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
        detail: "startup_resolution".to_owned(),
        evidence: None,
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
        let mut created = AuthorityAnchor::new(
            ledger
                .latch()
                .map_or(0, |latch| latch.token().generation().unwrap_or(0)),
            &token_hash,
            ledger.head_event_id(),
            ledger.chain_head(),
            ledger.checkpoint_event_id(),
            record_id,
        );
        if let Some(existing) = anchor
            && let Some(issue) = existing.bootstrap_issue.clone()
        {
            created.issue_bootstrap(issue);
            if let Some(issue) = created.bootstrap_issue.as_mut() {
                issue.consume();
            }
        }
        return anchors.atomic_write(&created);
    }
    let Some(existing) = anchor else {
        return Err(StorageError::PendingRefFailed);
    };
    let mut prepared = existing.clone();
    if prepared.generation == 0 && prepared.token_hash.is_empty() {
        prepared.generation = ledger
            .latch()
            .and_then(|latch| latch.token().generation())
            .unwrap_or(0);
        prepared.token_hash = ledger
            .latch()
            .map_or_else(String::new, |latch| latch.token().hash().to_owned());
        prepared.committed_head = ledger.head_event_id();
        prepared.journal_hash_chain_head = ledger.chain_head();
        prepared.checkpoint_event_id = ledger.checkpoint_event_id();
    }
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
