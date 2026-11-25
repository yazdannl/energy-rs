//! Sealed bootstrap transaction. Non-extendable: token, event, commit only.

use crate::authority::provenance::Provenance;
use crate::authority::{
    AnchorStore, AuthorityEventKind, AuthorityLedger, AuthorityState, BootstrapCapability,
    BootstrapReason, BootstrapTxError, CommittedBootstrap, InitializationOrRecoveryEvent,
    IssuedBootstrapCapability, LatchToken, WRITE_AUTHORITY_SCHEMA,
};

/// Non-extendable transaction scope. Private fields; no device, queue, settings or append API.
#[derive(Debug)]
pub struct BootstrapAuthorityTx {
    ledger: AuthorityLedger,
    expected_reason: BootstrapReason,
    previous_mode: Option<String>,
    source_release: String,
    at_ms: i64,
    actor: String,
    nonce: u64,
    token: Option<LatchToken>,
    event: Option<InitializationOrRecoveryEvent>,
}

impl BootstrapAuthorityTx {
    /// Begin only with an issued, unconsumed, run-bound bootstrap capability.
    pub(crate) fn begin(
        ledger: AuthorityLedger,
        at_ms: i64,
        actor: &str,
        anchors: &dyn AnchorStore,
        capability: &IssuedBootstrapCapability,
    ) -> Result<Self, BootstrapTxError> {
        let issue = anchors
            .load()
            .ok()
            .flatten()
            .and_then(|anchor| anchor.bootstrap_issue)
            .ok_or(BootstrapTxError::CapabilityDenied)?;
        if issue.consumed {
            return Err(BootstrapTxError::ReplayDenied);
        }
        if issue.run_id != capability.run_id()
            || issue.tx_id != capability.tx_id()
            || issue.nonce != capability.nonce
        {
            return Err(BootstrapTxError::CapabilityDenied);
        }
        let (reason, previous_mode) = match crate::authority::provenance::decide(&ledger) {
            Provenance::FreshInstall => (BootstrapReason::FreshInstall, None),
            Provenance::Legacy(reason) => (reason, ledger.persisted_mode().map(str::to_owned)),
            Provenance::Recovery => return Err(BootstrapTxError::ProvenanceAmbiguous),
        };
        let source_release = ledger.source_release().to_owned();
        let nonce = issue.nonce;
        Ok(Self {
            ledger,
            expected_reason: reason,
            previous_mode,
            source_release,
            at_ms,
            actor: actor.to_owned(),
            nonce,
            token: None,
            event: None,
        })
    }

    /// Attempt to reconstruct the capability from durable state; always a typed refusal.
    pub fn begin_external(
        _ledger: AuthorityLedger,
        _at_ms: i64,
        _actor: &str,
        anchors: &dyn AnchorStore,
    ) -> Result<Self, BootstrapTxError> {
        let issue = anchors
            .load()
            .ok()
            .flatten()
            .and_then(|anchor| anchor.bootstrap_issue);
        Err(if issue.is_some_and(|value| value.consumed) {
            BootstrapTxError::ReplayDenied
        } else {
            BootstrapTxError::CapabilityDenied
        })
    }

    /// The only token shape available in this transaction.
    pub fn token(&self) -> LatchToken {
        LatchToken::mint(1, &self.ledger.seed())
    }

    /// The only event shape available in this transaction.
    pub fn event(&self) -> InitializationOrRecoveryEvent {
        InitializationOrRecoveryEvent::new(
            self.expected_reason,
            1,
            self.previous_mode.clone(),
            self.source_release.clone(),
            self.at_ms,
            self.nonce,
        )
    }

    /// Write the narrowly typed token and generation.
    pub fn write_token(
        &mut self,
        token: LatchToken,
        generation: u64,
    ) -> Result<(), BootstrapTxError> {
        if self.token.is_some() || generation != 1 || token.generation() != Some(generation) {
            return Err(BootstrapTxError::TokenInvalid);
        }
        self.token = Some(token);
        Ok(())
    }

    /// Write the matching initialization/recovery event.
    pub fn write_event(
        &mut self,
        event: InitializationOrRecoveryEvent,
    ) -> Result<(), BootstrapTxError> {
        if self.event.is_some()
            || event.generation() != 1
            || event.nonce() != self.nonce
            || event.reason() != self.expected_reason
        {
            return Err(BootstrapTxError::EventInvalid);
        }
        self.event = Some(event);
        Ok(())
    }

    /// Runtime rejection for capabilities the language cannot name as absent.
    pub fn deny_extra_capability(&self, capability: BootstrapCapability) -> BootstrapTxError {
        let _ = capability;
        BootstrapTxError::CapabilityDenied
    }

    /// Commit token, event and schema marker in one value transaction.
    pub fn commit(self) -> Result<CommittedBootstrap, BootstrapTxError> {
        let token = self.token.ok_or(BootstrapTxError::TokenInvalid)?;
        let event = self.event.ok_or(BootstrapTxError::EventInvalid)?;
        if token.generation() != Some(event.generation()) {
            return Err(BootstrapTxError::GenerationMismatch);
        }
        let mut ledger = self.ledger;
        if ledger.has_initialization_event() {
            return Err(BootstrapTxError::ReplayDenied);
        }
        let reason = event.reason();
        let kind = AuthorityEventKind::LatchTokenInitialized {
            reason,
            generation: event.generation(),
            previous_mode: event.previous_mode().map(str::to_owned),
            source_release: event.source_release().to_owned(),
            nonce: event.nonce(),
        };
        ledger.set_schema_version(WRITE_AUTHORITY_SCHEMA);
        ledger.append(event.at_ms(), &self.actor, reason.as_str(), kind);
        let state = match reason {
            BootstrapReason::LegacyDryRunPreserved => AuthorityState::DryRunLatched,
            _ => AuthorityState::Auto,
        };
        let initial_reason = format!("{}:{}", reason.as_str(), event.nonce());
        ledger.set_latch(ledger.initial_latch(
            event.generation(),
            state,
            &initial_reason,
            event.at_ms(),
        ));
        let warning = match reason {
            BootstrapReason::LegacyAdvisoryMigratedToAuto => {
                "Advisory was compute-only and has been migrated to auto.".to_owned()
            }
            BootstrapReason::UnknownModeMigratedToAuto => format!(
                "Persisted mode {:?} was unknown and migrated to auto.",
                event.previous_mode().unwrap_or("missing")
            ),
            _ => String::new(),
        };
        Ok(CommittedBootstrap {
            ledger,
            token_hash: token.hash().to_owned(),
            generation: event.generation(),
            nonce: event.nonce(),
            reason,
            warning,
        })
    }
}
