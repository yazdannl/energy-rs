//! M6 bootstrap and sealed-transaction scenarios: S190-S194, S198, S199, S202-S205, S210.
//! Production lines caught by these regressions:
//! `BootstrapAuthorityTx::{begin,write_token,write_event,commit}` (audited provenance,
//! single-use commit, issued run/tx marker, no out-of-scope capability);
//! `startup_steps::bootstrap` and `startup_steps::phase_for` (no mode-based authority,
//! no silent repair, recovery survives a restart until authority reconcile).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod m6_support;

use energy_decide::authority::{
    AuthorityAnchor, AuthorityEventKind, AuthorityLedger, AuthorityPhase, AuthorityRefusal,
    AuthorityState, BootstrapCapability, BootstrapIssue, BootstrapReason, BootstrapTxError, Caller,
    EntryPoint, GateOutcome, StartupResolution, WriteClass, WriteRequest,
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
    let fixture = auto_fixture(1_000);
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
    let anchor = fixture.anchor().expect("anchor");
    assert_eq!(anchor.committed_head, 1);
    assert!(anchor.bootstrap_issue.is_some_and(|issue| issue.consumed));
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
    let booted = auto_fixture(4_000);
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
        GateOutcome::Refused(AuthorityRefusal::AuthorityRecoveryRequired)
    );
}

#[test]
fn s194_recovery_survives_repair_and_restart_until_authority_reconcile() {
    let booted = auto_fixture(5_000);
    let ledger = booted.authority.ledger();
    let without_anchor = Fixture::new(ledger.clone(), 5_010);
    assert_eq!(
        without_anchor.authority.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert!(!without_anchor.authority.startup().gate_open);
    assert_eq!(
        without_anchor
            .authority
            .authorize(&command("P", 800.0, 5_010)),
        AuthorityDecision::Denied {
            reason: "authority_recovery_required"
        }
    );

    let record_id = without_anchor
        .records()
        .first()
        .map_or(0, |record| record.id);
    let repaired = AuthorityAnchor::new(
        1,
        ledger.latch().map_or("", |latch| latch.token().hash()),
        ledger.head_event_id(),
        ledger.chain_head(),
        ledger.checkpoint_event_id(),
        record_id,
    );
    without_anchor.restore_anchor(&repaired);
    let restarted = without_anchor.restart(ledger, 5_020);
    assert_eq!(
        restarted.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert!(!restarted.startup().gate_open);

    let evidence = restarted.startup().evidence.expect("locked evidence");
    let permit = restarted
        .issue_reconcile_permit("operator", 5_030, "repaired anchor evidence")
        .expect("authority permit");
    restarted
        .reconcile_recovery(
            &without_anchor.anchors,
            &without_anchor.resolutions,
            permit,
            &evidence,
            5_040,
        )
        .expect("authority reconcile");
    assert_eq!(
        restarted.startup().resolution,
        StartupResolution::Reconciled
    );
    assert!(restarted.startup().gate_open);
    let after = without_anchor.restart(restarted.ledger(), 5_050);
    assert_eq!(after.startup().resolution, StartupResolution::Reconciled);
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
    assert!(
        energy_decide::authority::BootstrapAuthorityTx::begin_external(
            fixture.authority.ledger(),
            8_010,
            "api",
            &fixture.anchors,
        )
        .is_err()
    );
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
            reason: "authority_recovery_required"
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
        assert_eq!(
            fixture
                .authority
                .authorize_write(&WriteRequest::new(caller.clone(), class)),
            GateOutcome::Refused(AuthorityRefusal::AuthorityRecoveryRequired)
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
fn s210_bootstrap_capability_is_not_reconstructible_and_denials_are_typed() {
    let fixture = auto_fixture(12_000);
    let ledger = fixture.authority.ledger();
    assert_eq!(
        energy_decide::authority::BootstrapAuthorityTx::begin_external(
            ledger.clone(),
            12_010,
            "api",
            &fixture.anchors,
        )
        .expect_err("replay"),
        BootstrapTxError::ReplayDenied
    );

    let mut previous = fixture.anchor().expect("anchor");
    let mut issue = BootstrapIssue::new(3, 99, 7, 12_000);
    issue.consumed = false;
    previous.bootstrap_issue = Some(issue);
    fixture.restore_anchor(&previous);
    assert_eq!(
        energy_decide::authority::BootstrapAuthorityTx::begin_external(
            ledger.clone(),
            12_020,
            "api",
            &fixture.anchors,
        )
        .expect_err("previous run"),
        BootstrapTxError::CapabilityDenied
    );

    let caller = Caller::system(EntryPoint::ApiRequest, 12_030, "reconstruct seal");
    let refusal = fixture.authority.reject_bootstrap_reconstruction(caller);
    assert_eq!(refusal, AuthorityRefusal::BootstrapTxCapabilityDenied);
    let audit = fixture.authority.audit();
    let last = audit.last().expect("gate audit row");
    assert_eq!(last.detail, "bootstrap_tx_capability_denied");
    assert_eq!(last.entry, EntryPoint::ApiRequest);

    let denied = energy_decide::authority::BootstrapCapability::DeviceWrite;
    assert_eq!(
        denied,
        BootstrapCapability::DeviceWrite,
        "forbidden capabilities remain typed values"
    );
}
