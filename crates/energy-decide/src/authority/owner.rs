//! M6 write authority: one gate, auditable transitions and authority-only capabilities.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;

use crate::authority::{
    AnchorStore, AuditEvent, AuditLog, AuthorityEventKind, AuthorityLedger, AuthorityPhase,
    AuthorityRefusal, AuthorityState, GateOutcome, HealthReport, ResolutionStore, StartupOutcome,
    StartupResolution, WriteRequest, gate, health, startup,
};

pub(crate) struct AuthorityCore {
    pub(crate) ledger: AuthorityLedger,
    pub(crate) phase: AuthorityPhase,
    pub(crate) state: AuthorityState,
    pub(crate) resolution: StartupResolution,
    pub(crate) startup: StartupOutcome,
    pub(crate) audit: AuditLog,
    pub(crate) used_fences: HashSet<String>,
    pub(crate) clearance_seq: Option<u64>,
}

/// The only owner of write authority; every entry point asks this object.
pub struct Authority {
    core: Mutex<AuthorityCore>,
}

impl Authority {
    /// Resolve startup exactly once and build the authority gate.
    pub fn start(
        ledger: AuthorityLedger,
        anchors: &dyn AnchorStore,
        resolutions: &dyn ResolutionStore,
        now_ms: i64,
    ) -> Self {
        let mut audit = AuditLog::default();
        let input = crate::authority::startup_input::StartupInput {
            ledger,
            anchors,
            resolutions,
            now_ms,
        };
        let (startup, ledger) = startup::resolve(&input, &mut audit);
        Self {
            core: Mutex::new(AuthorityCore {
                ledger,
                phase: startup.phase,
                state: startup.state,
                resolution: startup.resolution,
                startup,
                audit,
                used_fences: HashSet::new(),
                clearance_seq: None,
            }),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, AuthorityCore> {
        self.core.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Effective operational authority state.
    pub fn state(&self) -> AuthorityState {
        self.lock().state
    }
    /// Effective authority phase.
    pub fn phase(&self) -> AuthorityPhase {
        self.lock().phase
    }
    /// Universal startup resolution.
    pub fn resolution(&self) -> StartupResolution {
        self.lock().resolution
    }
    /// Startup outcome including the banner and dispatch eligibility.
    pub fn startup(&self) -> StartupOutcome {
        self.lock().startup.clone()
    }
    /// Health payload mirrored by dashboard and API.
    pub fn health(&self) -> HealthReport {
        let core = self.lock();
        health::report(core.state, core.phase, core.resolution)
    }
    /// All gate decisions in order.
    pub fn audit(&self) -> Vec<AuditEvent> {
        self.lock().audit.events().to_vec()
    }
    /// Observable gate-invocation count used by the single-gate proof.
    pub fn gate_calls(&self) -> u64 {
        self.lock().audit.gate_calls()
    }
    /// Snapshot of the ledger value held by this process.
    pub fn ledger(&self) -> AuthorityLedger {
        self.lock().ledger.clone()
    }

    /// The one decision function; all public authorization APIs delegate here.
    pub(crate) fn decide(&self, request: &WriteRequest) -> GateOutcome {
        let mut core = self.lock();
        let outcome = gate::decide(
            request,
            core.phase,
            core.state,
            core.resolution,
            &core.used_fences,
        );
        let detail = detail_for(&outcome, core.state);
        let event = AuditEvent {
            seq: 0,
            at_ms: request.caller.at_ms(),
            actor: request.caller.actor().to_owned(),
            entry: request.caller.entry(),
            class: request.class.clone(),
            state: core.state,
            phase: core.phase,
            outcome: outcome.clone(),
            why: request.caller.why().to_owned(),
            detail,
        };
        core.audit.record(event);
        if outcome.authorized() {
            if let crate::authority::WriteClass::DeviceNeutralisation { fence_id } = &request.class
            {
                core.used_fences.insert(fence_id.clone());
            }
        } else if core.phase == AuthorityPhase::Initialized
            && core.state == AuthorityState::DryRunLatched
        {
            let _ = core.ledger.append(
                request.caller.at_ms(),
                request.caller.actor(),
                "dry_run_latch_preserved",
                AuthorityEventKind::DryRunLatchPreserved {
                    path: request.caller.entry().as_str().to_owned(),
                },
            );
        }
        outcome
    }
}

fn detail_for(outcome: &GateOutcome, state: AuthorityState) -> &'static str {
    match outcome {
        GateOutcome::Authorized => "authorized",
        GateOutcome::Refused(AuthorityRefusal::PersistenceDenied(_)) => "persistence_denied",
        GateOutcome::Refused(AuthorityRefusal::WriteDeniedByAuthorityGate) => state.as_str(),
        GateOutcome::Refused(refusal) => refusal.slug(),
    }
}
