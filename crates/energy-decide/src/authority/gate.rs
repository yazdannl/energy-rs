//! THE single write-authority gate. Every entry point calls `decide`; no local flags exist.

use std::collections::HashSet;

use crate::authority::{
    AuthorityPhase, AuthorityRefusal, AuthorityState, EntryPoint, GateOutcome, StartupResolution,
    WriteClass, WriteRequest,
};

/// Decide one write request from the M5 state only, with phase and resolution as hard gates.
pub(crate) fn decide(
    request: &WriteRequest,
    phase: AuthorityPhase,
    state: AuthorityState,
    resolution: StartupResolution,
    used_fences: &HashSet<String>,
) -> GateOutcome {
    // The append-only diagnostic record is writable in every phase, including invalid.
    if request.class == WriteClass::StartupResolution {
        return GateOutcome::Authorized;
    }
    if request.class == WriteClass::CheckpointWrite {
        return GateOutcome::Refused(AuthorityRefusal::CheckpointWriteDenied);
    }
    if request.class == WriteClass::OperationalDryRun {
        return GateOutcome::Refused(AuthorityRefusal::DryRunNotOperational);
    }
    if request.class == WriteClass::BootstrapCapability {
        return GateOutcome::Refused(AuthorityRefusal::BootstrapTxCapabilityDenied);
    }
    if let WriteClass::ReconcileAuthorization { valid } = request.class {
        return if valid
            && request.caller.entry() == EntryPoint::InternalRecovery
            && request.caller.is_authority_internal()
        {
            GateOutcome::Authorized
        } else {
            GateOutcome::Refused(AuthorityRefusal::ReconcileDenied)
        };
    }
    // Recovery-required outranks an invalid phase: consumers must always see the
    // actionable verdict, with the failing evidence carried in the audit detail.
    if resolution == StartupResolution::RecoveryRequired {
        return GateOutcome::Refused(AuthorityRefusal::AuthorityRecoveryRequired);
    }
    if phase == AuthorityPhase::Uninitialized {
        return GateOutcome::Refused(AuthorityRefusal::AuthorityUninitialized);
    }
    if phase == AuthorityPhase::Invalid {
        return GateOutcome::Refused(AuthorityRefusal::LatchTokenInvalid);
    }
    if let WriteClass::ClearanceAuthorization { valid } = request.class {
        return if valid {
            GateOutcome::Authorized
        } else {
            GateOutcome::Refused(AuthorityRefusal::ClearanceAuthorizationInvalid)
        };
    }
    let outcome = decide_initialized(request, state, used_fences);
    if !outcome.authorized()
        && request.caller.entry() == EntryPoint::QueuedIntentExecution
        && request.class.is_device()
    {
        return GateOutcome::Refused(AuthorityRefusal::IntentNotAuthorized);
    }
    outcome
}

fn decide_initialized(
    request: &WriteRequest,
    state: AuthorityState,
    used_fences: &HashSet<String>,
) -> GateOutcome {
    match &request.class {
        WriteClass::ControlPlaneJournal | WriteClass::StartupResolution => GateOutcome::Authorized,
        WriteClass::ControlPlaneAuthority => {
            if request.caller.entry() == EntryPoint::InternalRecovery
                && !request.caller.is_authority_internal()
            {
                GateOutcome::Refused(AuthorityRefusal::ReconcileDenied)
            } else if request.caller.is_authority_internal()
                || request.caller.entry() == EntryPoint::ClearanceCommand
            {
                GateOutcome::Authorized
            } else {
                GateOutcome::Refused(AuthorityRefusal::AuthorityTransitionDenied)
            }
        }
        WriteClass::DomainPersistence => match state {
            AuthorityState::Auto => GateOutcome::Authorized,
            AuthorityState::DryRunLatched | AuthorityState::Clearing => {
                GateOutcome::Refused(AuthorityRefusal::PersistenceDenied(state))
            }
        },
        WriteClass::DeviceEconomic => match state {
            AuthorityState::Auto => GateOutcome::Authorized,
            AuthorityState::DryRunLatched | AuthorityState::Clearing => {
                GateOutcome::Refused(AuthorityRefusal::WriteDeniedByAuthorityGate)
            }
        },
        WriteClass::DeviceNeutralisation { fence_id } => {
            decide_neutralisation(state, fence_id, used_fences)
        }
        WriteClass::OperationalDryRun
        | WriteClass::ClearanceAuthorization { .. }
        | WriteClass::ReconcileAuthorization { .. }
        | WriteClass::BootstrapCapability
        | WriteClass::CheckpointWrite => {
            GateOutcome::Refused(AuthorityRefusal::WriteDeniedByAuthorityGate)
        }
    }
}

fn decide_neutralisation(
    state: AuthorityState,
    fence_id: &str,
    used_fences: &HashSet<String>,
) -> GateOutcome {
    match state {
        AuthorityState::DryRunLatched => {
            GateOutcome::Refused(AuthorityRefusal::WriteDeniedByAuthorityGate)
        }
        AuthorityState::Clearing => {
            if fence_id.is_empty() || used_fences.contains(fence_id) {
                GateOutcome::Refused(AuthorityRefusal::FreshFenceRequired)
            } else {
                GateOutcome::Authorized
            }
        }
        AuthorityState::Auto => GateOutcome::Authorized,
    }
}
