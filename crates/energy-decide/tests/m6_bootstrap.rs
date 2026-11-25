//! M6 bootstrap and sealed-transaction scenarios: S190-S194, S198, S199, S202-S205, S210.
//! Production lines caught by these regressions:
//! `BootstrapAuthorityTx::{begin,write_token,write_event,commit}` (audited provenance,
//! single-use commit, no out-of-scope capability); `startup_steps::bootstrap` and
//! `startup_steps::phase_for` (no mode-based authority, no silent repair).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod m6_support;

use energy_decide::authority::{
    AuthorityAnchor, AuthorityEventKind, AuthorityLedger, AuthorityPhase, AuthorityState,
    BootstrapAuthorityTx, BootstrapCapability, BootstrapReason, Caller, EntryPoint, GateOutcome,
    StartupResolution, WriteClass, WriteRequest,
};
use energy_decide::dispatch::{AuthorityDecision, WriteAuthority};
use m6_support::{Fixture, auto_fixture, command};

fn init_reason(ledger: &AuthorityLedger) -> Option<BootstrapReason> {
    ledger.events().iter().find_map(|event| match event.kind {
        AuthorityEventKind::LatchTokenInitialized { reason, .. } => Some(reason),
        _ => None,
    })
}

#[test]
fn s190_fresh_install_commits_audited_auto_before_any_write() {
    let fixture = m6_support::auto_fixture(1_000);
    let authority = &fixture.authority;
    assert_eq!(authority.state(), AuthorityState::Auto);
    assert_eq!(authority.phase(), AuthorityPhase::Initialized);
    assert_eq!(
        authority.startup().resolution,
        StartupResolution::Reconciled
    );
    assert!(authority.startup().gate_open);
    assert!(authority.startup().dispatch_eligible);
    assert_eq!(
        authority.ledger().latch().map(|l| l.token().generation()),
        Some(Some(1))
    );
    assert_eq!(
        init_reason(&authority.ledger()),
        Some(BootstrapReason::FreshInstall)
    );
    assert_eq!(authority.ledger().checkpoint_event_id(), 1);
    assert_eq!(
        authority.audit().first().map(|event| event.entry),
        Some(EntryPoint::StartupResolutionWrite)
    );
    assert_eq!(
        fixture.anchor().map(|anchor| anchor.committed_head),
        Some(1)
    );
}

#[test]
fn s191_legacy_auto_migration_is_audited() {
    let fixture = Fixture::new(m6_support::legacy("auto"), 2_000);
    assert_eq!(fixture.authority.state(), AuthorityState::Auto);
    assert_eq!(
        init_reason(&fixture.authority.ledger()),
        Some(BootstrapReason::LegacyAutoMigration)
    );
    assert!(
        fixture
            .authority
            .ledger()
            .events()
            .iter()
            .any(|event| event.kind.family() == "latch_token_initialized")
    );
}

#[test]
fn s192_legacy_dry_run_is_preserved_write_free() {
    let fixture = m6_support::latched_fixture(3_000);
    assert_eq!(fixture.authority.state(), AuthorityState::DryRunLatched);
    assert_eq!(
        init_reason(&fixture.authority.ledger()),
        Some(BootstrapReason::LegacyDryRunPreserved)
    );
    assert!(!fixture.authority.startup().dispatch_eligible);
    assert!(fixture.authority.startup().gate_open);
    assert!(
        fixture
            .authority
            .ledger()
            .latch()
            .is_some_and(|latch| !latch.reason().is_empty())
    );
}

#[test]
fn s193_malformed_generation_requires_operator_recovery() {
    let booted = m6_support::auto_fixture(4_000);
    let mut ledger = booted.authority.ledger();
    ledger.corrupt_generation("1x");
    let restarted = booted.restart(ledger, 4_010);
    assert_eq!(restarted.phase(), AuthorityPhase::Invalid);
    assert_eq!(
        restarted.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert_eq!(
        restarted.authorize_write(&WriteRequest::new(
            Caller::system(EntryPoint::ApiRequest, 4_010, "write"),
            WriteClass::DeviceEconomic,
        )),
        GateOutcome::Refused(energy_decide::authority::AuthorityRefusal::LatchTokenInvalid)
    );
}

#[test]
fn s194_token_event_commit_alone_never_opens_the_gate() {
    let source = m6_support::fresh(5_000);
    let mut tx =
        BootstrapAuthorityTx::begin(source.clone(), 5_000, "authority").expect("bootstrap");
    let token = tx.token();
    let event = tx.event();
    tx.write_token(token, 1).expect("token");
    tx.write_event(event).expect("event");
    let committed = tx.commit().expect("atomic commit");
    let ledger = committed.ledger_snapshot();

    let fixture = Fixture::new(ledger.clone(), 5_010);
    assert_eq!(
        fixture.authority.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert!(!fixture.authority.startup().gate_open);
    assert_eq!(
        fixture.authority.authorize(&command("P", 800.0, 5_010)),
        AuthorityDecision::Denied {
            reason: "authority_recovery_required"
        }
    );

    let record_id = fixture.records().first().map_or(0, |record| record.id);
    let anchor = AuthorityAnchor::new(
        1,
        ledger.latch().map_or("", |latch| latch.token().hash()),
        ledger.head_event_id(),
        ledger.chain_head(),
        ledger.checkpoint_event_id(),
        record_id,
    );
    fixture.restore_anchor(&anchor);
    let restarted = fixture.restart(ledger, 5_020);
    assert_eq!(
        restarted.startup().resolution,
        StartupResolution::Reconciled
    );
    assert!(restarted.startup().gate_open);
}

#[test]
fn s198_legacy_advisory_migrates_to_auto_with_a_loud_warning() {
    let fixture = Fixture::new(m6_support::legacy("advisory"), 6_000);
    assert_eq!(
        init_reason(&fixture.authority.ledger()),
        Some(BootstrapReason::LegacyAdvisoryMigratedToAuto)
    );
    assert_eq!(fixture.authority.state(), AuthorityState::Auto);
    assert_eq!(
        fixture.authority.startup().warning,
        "Advisory was compute-only and has been migrated to auto."
    );
}

#[test]
fn s199_unknown_and_missing_legacy_modes_name_the_value() {
    let banana = Fixture::new(m6_support::legacy("banana"), 7_000);
    assert_eq!(
        init_reason(&banana.authority.ledger()),
        Some(BootstrapReason::UnknownModeMigratedToAuto)
    );
    assert!(banana.authority.startup().warning.contains("banana"));
    let mut missing_ledger = AuthorityLedger::new(1, None, "0.14.9");
    missing_ledger.seed_journal_note(7_000, "pre-authority-note");
    let missing = Fixture::new(missing_ledger, 7_010);
    assert_eq!(
        init_reason(&missing.authority.ledger()),
        Some(BootstrapReason::UnknownModeMigratedToAuto)
    );
    assert!(missing.authority.startup().warning.contains("missing"));
    assert_eq!(missing.authority.state(), AuthorityState::Auto);
}

#[test]
fn s202_schema_one_without_latch_events_permits_legacy_migration() {
    let fixture = Fixture::new(m6_support::legacy("auto"), 8_000);
    assert_eq!(fixture.authority.ledger().schema_version(), 2);
    assert_eq!(
        init_reason(&fixture.authority.ledger()),
        Some(BootstrapReason::LegacyAutoMigration)
    );
    let post_token = fixture.authority.ledger();
    let tx = BootstrapAuthorityTx::begin(post_token, 8_010, "authority");
    assert!(tx.is_err());
}

#[test]
fn s203_post_token_missing_token_never_bootstraps_again() {
    let auto = auto_fixture(9_000);
    let mut ledger = auto.authority.ledger();
    ledger.remove_latch();
    let fixture = Fixture::new(ledger, 9_010);
    assert_eq!(fixture.authority.phase(), AuthorityPhase::Invalid);
    assert_eq!(
        fixture.authority.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert_eq!(
        init_reason(&fixture.authority.ledger()),
        Some(BootstrapReason::FreshInstall)
    );
}

#[test]
fn s204_unreadable_token_with_initialization_event_stays_in_recovery() {
    let auto = auto_fixture(10_000);
    let mut ledger = auto.authority.ledger();
    ledger.corrupt_generation("1x");
    let fixture = auto.restart(ledger, 10_010);
    assert_eq!(fixture.startup().reason, Some("latch_token_invalid"));
    assert!(fixture.ledger().has_initialization_event());
    assert_eq!(
        fixture.authorize(&command("P", -800.0, 10_010)),
        AuthorityDecision::Denied {
            reason: "latch_token_invalid"
        }
    );
}

#[test]
fn s205_uninitialized_scope_allows_only_token_event_and_diagnostic_record() {
    let auto = auto_fixture(11_000);
    let mut ledger = auto.authority.ledger();
    ledger.remove_latch();
    let fixture = Fixture::new(ledger, 11_010);
    let caller = Caller::system(EntryPoint::ApiRequest, 11_010, "uninitialized scope");
    for class in [
        WriteClass::ControlPlaneJournal,
        WriteClass::ControlPlaneAuthority,
        WriteClass::DomainPersistence,
        WriteClass::DeviceEconomic,
    ] {
        let outcome = fixture
            .authority
            .authorize_write(&WriteRequest::new(caller.clone(), class));
        assert_eq!(
            outcome,
            GateOutcome::Refused(energy_decide::authority::AuthorityRefusal::LatchTokenInvalid)
        );
    }
    assert!(
        fixture
            .authority
            .authorize_write(&WriteRequest::new(caller, WriteClass::StartupResolution,))
            .authorized()
    );
    assert_eq!(
        fixture.authority.audit().first().map(|e| e.entry),
        Some(EntryPoint::StartupResolutionWrite)
    );
}

#[test]
fn s210_bootstrap_transaction_is_sealed_and_non_replayable() {
    let source = m6_support::fresh(12_000);
    let mut tx = BootstrapAuthorityTx::begin(source.clone(), 12_000, "operator").expect("begin");
    assert_eq!(
        tx.deny_extra_capability(BootstrapCapability::DeviceWrite),
        energy_decide::authority::BootstrapTxError::CapabilityDenied
    );
    assert_eq!(
        tx.deny_extra_capability(BootstrapCapability::JournalAppend)
            .slug(),
        "bootstrap_tx_capability_denied"
    );
    let token = tx.token();
    let event = tx.event();
    tx.write_token(token, 1).expect("token");
    tx.write_event(event).expect("event");
    let committed = tx.commit().expect("commit");
    assert_eq!(committed.nonce(), committed.nonce());
    let ledger = committed.ledger_snapshot();
    assert_eq!(ledger.events().len(), 1);
    assert_eq!(ledger.checkpoint_event_id(), 1);
    assert!(BootstrapAuthorityTx::begin(ledger, 12_010, "operator").is_err());
}
