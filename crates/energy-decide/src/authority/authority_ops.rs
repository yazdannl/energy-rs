//! Narrow authorization operations; every one delegates to the single gate first.

use crate::authority::{
    Authority, AuthorityEventKind, AuthorityRefusal, AuthorityState, Caller, ClearanceCommand,
    ClearancePermit, EntryPoint, GateOutcome, IntentOutcome, QueuedIntent, ReconcilePermit,
    WriteClass, WriteRequest,
};
use crate::dispatch::{AuthorityDecision, PackCommand, WriteAuthority};

fn refusal(outcome: &GateOutcome) -> AuthorityRefusal {
    match outcome {
        GateOutcome::Authorized => AuthorityRefusal::WriteDeniedByAuthorityGate,
        GateOutcome::Refused(reason) => reason.clone(),
    }
}

impl Authority {
    /// General entry point wrapper; the request alone decides the answer.
    pub fn authorize_write(&self, request: &WriteRequest) -> GateOutcome {
        self.decide(request)
    }

    /// Persistence entry points (learning, maintenance, dashboard, attempt outcomes).
    pub fn authorize_persistence(&self, caller: Caller, class: WriteClass) -> GateOutcome {
        self.decide(&WriteRequest::new(caller, class))
    }

    /// Generic websocket passthrough; no settings write can establish authority here.
    pub fn authorize_ws_passthrough(&self, caller: Caller, class: WriteClass) -> GateOutcome {
        self.decide(&WriteRequest::new(caller, class))
    }

    /// Boot-time neutralisation entry point, using the real startup caller identity.
    pub fn authorize_boot_neutralisation(&self, at_ms: i64, fence_id: &str) -> GateOutcome {
        self.decide(&WriteRequest::new(
            Caller::system(EntryPoint::Startup, at_ms, "boot neutralisation"),
            WriteClass::DeviceNeutralisation {
                fence_id: fence_id.to_owned(),
            },
        ))
    }

    /// Shutdown/watchdog neutralisation entry point.
    pub fn authorize_shutdown_neutralisation(&self, at_ms: i64, fence_id: &str) -> GateOutcome {
        self.decide(&WriteRequest::new(
            Caller::system(
                EntryPoint::WatchdogRestart,
                at_ms,
                "shutdown neutralisation",
            ),
            WriteClass::DeviceNeutralisation {
                fence_id: fence_id.to_owned(),
            },
        ))
    }

    /// Settings/mode persistence entry point; an operator cannot establish authority here.
    pub fn authorize_settings_write(&self, actor: &str, at_ms: i64, why: &str) -> GateOutcome {
        self.decide(&WriteRequest::new(
            Caller::operator(EntryPoint::DashboardMutation, actor, at_ms, why),
            WriteClass::DomainPersistence,
        ))
    }

    /// Operational `dry_run` requests are rejected with a typed 4xx reason.
    pub fn decline_operational_dry_run(&self, caller: Caller) -> AuthorityRefusal {
        refusal(&self.decide(&WriteRequest::new(caller, WriteClass::OperationalDryRun)))
    }

    /// Enter `clearing` first, with a logged command, barrier generation and audit entry.
    pub fn begin_clearance(
        &self,
        command: &ClearanceCommand,
    ) -> Result<ClearancePermit, AuthorityRefusal> {
        let caller = Caller::operator(
            EntryPoint::ClearanceCommand,
            &command.actor,
            command.at_ms,
            "operator clearance",
        );
        let outcome = self.decide(&WriteRequest::new(
            caller,
            WriteClass::ClearanceAuthorization {
                valid: command.valid(),
            },
        ));
        if !outcome.authorized() {
            return Err(refusal(&outcome));
        }
        let seq = self.gate_calls();
        let mut core = self.lock();
        let generation = core
            .ledger
            .latch()
            .and_then(|latch| latch.token().generation())
            .unwrap_or(0)
            .saturating_add(1);
        core.ledger.append(
            command.at_ms,
            &command.actor,
            "clearance_entry",
            AuthorityEventKind::ClearanceEntry {
                clearance_id: command.clearance_id.clone(),
                generation,
            },
        );
        if let Some(latch) = core.ledger.latch().cloned() {
            let mut latch = latch;
            latch.transition(
                AuthorityState::Clearing,
                "logged_operator_clearance",
                command.at_ms,
            );
            core.ledger.set_latch(latch);
        }
        core.state = AuthorityState::Clearing;
        core.clearance_seq = Some(seq);
        Ok(ClearancePermit::new(
            command.clearance_id.clone(),
            generation,
            command.at_ms,
            seq,
        ))
    }

    /// Commit `auto` only after all packs are neutral, in one audited transition.
    pub fn commit_clearance(
        &self,
        permit: &ClearancePermit,
        at_ms: i64,
    ) -> Result<(), AuthorityRefusal> {
        let caller = Caller::operator(
            EntryPoint::ClearanceCommand,
            "authority",
            at_ms,
            "clearance completed",
        );
        let outcome = self.decide(&WriteRequest::new(
            caller,
            WriteClass::ControlPlaneAuthority,
        ));
        if !outcome.authorized() {
            return Err(refusal(&outcome));
        }
        let mut core = self.lock();
        if core.state != AuthorityState::Clearing || core.clearance_seq != Some(permit.seq()) {
            return Err(AuthorityRefusal::AuthorityTransitionDenied);
        }
        core.ledger.append(
            at_ms,
            "authority",
            "clearance_completed",
            AuthorityEventKind::AuthorityTransition {
                from: AuthorityState::Clearing,
                to: AuthorityState::Auto,
                reason: permit.clearance_id().to_owned(),
                clearance_id: Some(permit.clearance_id().to_owned()),
            },
        );
        if let Some(latch) = core.ledger.latch().cloned() {
            let mut latch = latch;
            latch.transition(AuthorityState::Auto, "clearance_completed", at_ms);
            core.ledger.set_latch(latch);
        }
        core.state = AuthorityState::Auto;
        core.clearance_seq = None;
        Ok(())
    }

    /// Failure or timeout returns to the write-free latch with the typed reason recorded.
    pub fn fail_clearance(
        &self,
        permit: &ClearancePermit,
        reason: &str,
        at_ms: i64,
    ) -> Result<(), AuthorityRefusal> {
        let caller = Caller::operator(
            EntryPoint::ClearanceCommand,
            "authority",
            at_ms,
            "clearance failed",
        );
        let outcome = self.decide(&WriteRequest::new(
            caller,
            WriteClass::ControlPlaneAuthority,
        ));
        if !outcome.authorized() {
            return Err(refusal(&outcome));
        }
        let mut core = self.lock();
        if core.state != AuthorityState::Clearing || core.clearance_seq != Some(permit.seq()) {
            return Err(AuthorityRefusal::AuthorityTransitionDenied);
        }
        core.ledger.append(
            at_ms,
            "authority",
            reason,
            AuthorityEventKind::AuthorityTransition {
                from: AuthorityState::Clearing,
                to: AuthorityState::DryRunLatched,
                reason: reason.to_owned(),
                clearance_id: Some(permit.clearance_id().to_owned()),
            },
        );
        if let Some(latch) = core.ledger.latch().cloned() {
            let mut latch = latch;
            latch.transition(AuthorityState::DryRunLatched, reason, at_ms);
            core.ledger.set_latch(latch);
        }
        core.state = AuthorityState::DryRunLatched;
        core.clearance_seq = None;
        Ok(())
    }

    /// Authority-only reconcile capability; ordinary callers are refused by the gate.
    pub fn authorize_reconcile(
        &self,
        caller: &Caller,
    ) -> Result<ReconcilePermit, AuthorityRefusal> {
        let before = self.gate_calls();
        let outcome = self.decide(&WriteRequest::new(
            caller.clone(),
            WriteClass::ReconcileAuthorization { valid: true },
        ));
        if !outcome.authorized() {
            return Err(refusal(&outcome));
        }
        Ok(ReconcilePermit::new(
            before.saturating_add(1),
            caller.at_ms(),
            caller.actor(),
            caller.why(),
        ))
    }

    /// Wiring boundary for a trusted authority reconcile; an API caller cannot forge this.
    pub fn issue_reconcile_permit(
        &self,
        actor: &str,
        at_ms: i64,
        why: &str,
    ) -> Result<ReconcilePermit, AuthorityRefusal> {
        self.authorize_reconcile(&Caller::internal_named(
            EntryPoint::InternalRecovery,
            actor,
            at_ms,
            why,
        ))
    }

    /// Re-check a queued intent at execution time; refusals stay observable rows.
    pub fn execute_intent(&self, intent: QueuedIntent) -> IntentOutcome {
        let class = command_class(&intent.command);
        let caller = Caller::system(
            EntryPoint::QueuedIntentExecution,
            intent.command.issued_at_ms,
            "queued device intent",
        );
        let outcome = self.decide(&WriteRequest::new(caller.clone(), class));
        match outcome {
            GateOutcome::Authorized => IntentOutcome::Authorized {
                command: intent.command,
            },
            GateOutcome::Refused(reason) => IntentOutcome::Refused {
                intent_id: intent.id,
                reason,
                entry: caller.entry(),
            },
        }
    }
}

pub fn command_class(command: &PackCommand) -> WriteClass {
    if command.safety_traffic
        && let Some(fence_id) = &command.fence_id
    {
        return WriteClass::DeviceNeutralisation {
            fence_id: fence_id.clone(),
        };
    }
    WriteClass::DeviceEconomic
}

impl WriteAuthority for Authority {
    fn authorize(&self, command: &PackCommand) -> AuthorityDecision {
        let caller = Caller::system(
            EntryPoint::TickDispatch,
            command.issued_at_ms,
            "m4 execution-time gate",
        );
        match self.decide(&WriteRequest::new(caller, command_class(command))) {
            GateOutcome::Authorized => AuthorityDecision::Authorized,
            GateOutcome::Refused(reason) => AuthorityDecision::Denied {
                reason: reason.slug(),
            },
        }
    }
}
