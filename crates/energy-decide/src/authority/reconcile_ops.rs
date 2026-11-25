//! Authority-only recovery reconcile: consumes the permit and clears the durable lock.

use crate::authority::{
    AnchorStore, Authority, AuthorityAnchor, AuthorityCore, AuthorityEventKind, AuthorityPhase,
    AuthorityRefusal, AuthorityState, Caller, EntryPoint, GateOutcome, ReconcilePermit,
    RecoveryEvidence, ResolutionStore, StartupResolution, WriteClass, WriteRequest, health,
    startup_evidence as evidence_steps,
};

impl Authority {
    /// Clear a locked recovery verdict with an authority-issued reconcile permit.
    ///
    /// No restart, mode write or timeout can call this; the permit is consumed here.
    pub fn reconcile_recovery(
        &self,
        anchors: &dyn AnchorStore,
        resolutions: &dyn ResolutionStore,
        permit: ReconcilePermit,
        evidence: &RecoveryEvidence,
        at_ms: i64,
    ) -> Result<(), AuthorityRefusal> {
        self.authorize_reconcile_call(at_ms)?;
        let result = self.apply_reconcile(anchors, resolutions, &permit, evidence, at_ms);
        drop(permit);
        result
    }
}

impl Authority {
    fn authorize_reconcile_call(&self, at_ms: i64) -> Result<(), AuthorityRefusal> {
        let caller = Caller::internal(
            EntryPoint::InternalRecovery,
            at_ms,
            "authority reconcile of repaired evidence",
        );
        let outcome = self.decide(&WriteRequest::new(
            caller,
            WriteClass::ReconcileAuthorization { valid: true },
        ));
        match outcome {
            GateOutcome::Authorized => Ok(()),
            GateOutcome::Refused(reason) => Err(reason),
        }
    }

    fn apply_reconcile(
        &self,
        anchors: &dyn AnchorStore,
        resolutions: &dyn ResolutionStore,
        permit: &ReconcilePermit,
        evidence: &RecoveryEvidence,
        at_ms: i64,
    ) -> Result<(), AuthorityRefusal> {
        let mut core = self.lock();
        let freshly_granted = core.audit.events().last().is_some_and(|event| {
            event.entry == EntryPoint::InternalRecovery
                && event.class == WriteClass::ReconcileAuthorization { valid: true }
                && event.outcome.authorized()
        });
        if core.resolution != StartupResolution::RecoveryRequired || !freshly_granted {
            return Err(AuthorityRefusal::ReconcileDenied);
        }
        let mut anchor = anchors
            .load()
            .map_err(|_| AuthorityRefusal::AnchorMismatch)?;
        let anchor = anchor.as_mut().ok_or(AuthorityRefusal::AnchorMismatch)?;
        if anchor.recovery.as_ref() != Some(evidence) {
            return Err(AuthorityRefusal::AnchorMismatch);
        }
        evidence_steps::append_reconcile_record(resolutions, at_ms)
            .map_err(|_| AuthorityRefusal::ResolutionRecordCorrupt)?;
        anchor.clear_recovery();
        record_reconcile(&mut core, anchor, permit, evidence, at_ms);
        anchors
            .atomic_write(anchor)
            .map_err(|_| AuthorityRefusal::AnchorMismatch)?;
        Ok(())
    }
}

fn record_reconcile(
    core: &mut AuthorityCore,
    anchor: &mut AuthorityAnchor,
    permit: &ReconcilePermit,
    evidence: &RecoveryEvidence,
    at_ms: i64,
) {
    core.ledger.append(
        at_ms,
        permit.actor(),
        "authority_recovery_reconciled",
        AuthorityEventKind::RecoveryReconciled {
            who: permit.actor().to_owned(),
            why: permit.why().to_owned(),
            evidence: evidence.detail(),
        },
    );
    if anchor.generation == 0 && anchor.token_hash.is_empty() {
        anchor.generation = core
            .ledger
            .latch()
            .and_then(|latch| latch.token().generation())
            .unwrap_or(0);
        anchor.token_hash = core
            .ledger
            .latch()
            .map_or_else(String::new, |latch| latch.token().hash().to_owned());
    }
    anchor.committed_head = core.ledger.head_event_id();
    anchor.journal_hash_chain_head = core.ledger.chain_head();
    anchor.checkpoint_event_id = core.ledger.checkpoint_event_id();
    core.resolution = StartupResolution::Reconciled;
    core.phase = AuthorityPhase::Initialized;
    core.state = core.ledger.latch().map_or(
        AuthorityState::DryRunLatched,
        crate::authority::Latch::state,
    );
    core.startup.resolution = StartupResolution::Reconciled;
    core.startup.state = core.state;
    core.startup.phase = AuthorityPhase::Initialized;
    core.startup.gate_open = true;
    core.startup.dispatch_eligible = core.state == AuthorityState::Auto;
    core.startup.health = health::report(core.state, core.phase, core.resolution, None);
    core.recovery_evidence = None;
}
