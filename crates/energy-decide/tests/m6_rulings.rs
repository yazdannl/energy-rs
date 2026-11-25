//! Ruling tests S319-S328: recovery verdict, sealed capability, permit boundary, coverage.
//! Production lines caught by these regressions:
//! `gate::decide` (recovery outranks invalid phase), `health::report` (distinct loud verdict),
//! `startup::{resolve,finish}` (durable lock), `authority_ops::reconcile_recovery`,
//! `BootstrapAuthorityTx::begin_external`, `Dispatcher::reconcile`.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod m4_support;
mod m6_support;

use std::sync::Arc;

use energy_decide::authority::{
    Authority, AuthorityRefusal, AuthorityState, BootstrapIssue, BootstrapTxError, Caller,
    ClearanceCommand, EntryPoint, GateOutcome, RecoveryEvidence, StartupResolution, WriteClass,
    WriteRequest,
};
use energy_decide::dispatch::{
    AllowAllAuthority, AuthorityDecision, DispatchState, Dispatcher, ProceedSafety, WriteAuthority,
};
use m6_support::{auto_fixture, command, fenced_hold};

fn refused_reason(outcome: GateOutcome) -> String {
    match outcome {
        GateOutcome::Authorized => "authorized".to_owned(),
        GateOutcome::Refused(reason) => reason.slug().to_owned(),
    }
}

#[test]
fn s319_recovery_survives_restart_and_clears_only_by_authority_reconcile() {
    let fixture = auto_fixture(20_000);
    std::fs::write(fixture.anchors.path(), "{not-json").expect("corrupt anchor");
    let first = fixture.restart(fixture.authority.ledger(), 20_010);
    assert_eq!(first.resolution(), StartupResolution::RecoveryRequired);
    assert_eq!(first.state(), AuthorityState::DryRunLatched);
    assert!(!first.startup().gate_open);
    let evidence = first.startup().evidence.expect("evidence");
    assert!(evidence.detail().contains("resolution_record_corrupt"));

    let second = fixture.restart(first.ledger(), 20_020);
    assert_eq!(second.resolution(), StartupResolution::RecoveryRequired);
    assert_eq!(
        refused_reason(second.authorize_write(&WriteRequest::new(
            Caller::system(EntryPoint::ApiRequest, 20_020, "after restart"),
            WriteClass::DeviceEconomic,
        ))),
        "authority_recovery_required"
    );
    let locked = second.startup().evidence.expect("locked evidence");
    let permit = second
        .issue_reconcile_permit("operator", 20_030, "anchor restored by operator")
        .expect("permit");
    second
        .reconcile_recovery(
            &fixture.anchors,
            &fixture.resolutions,
            permit,
            &locked,
            20_040,
        )
        .expect("reconcile");
    assert_eq!(second.startup().resolution, StartupResolution::Reconciled);
    let after = fixture.restart(second.ledger(), 20_050);
    assert_eq!(after.startup().resolution, StartupResolution::Reconciled);
}

#[test]
fn s320_recovery_is_loud_distinct_and_typed_even_in_invalid_phase() {
    let fixture = auto_fixture(21_000);
    let mut ledger = fixture.authority.ledger();
    ledger.corrupt_generation("1x");
    let restarted = fixture.restart(ledger, 21_010);
    assert_eq!(
        restarted.phase(),
        energy_decide::authority::AuthorityPhase::Invalid
    );
    assert_eq!(restarted.resolution(), StartupResolution::RecoveryRequired);
    for class in [
        WriteClass::DeviceEconomic,
        WriteClass::ControlPlaneAuthority,
        WriteClass::DomainPersistence,
        WriteClass::DeviceNeutralisation {
            fence_id: "fence-21".to_owned(),
        },
    ] {
        assert_eq!(
            refused_reason(restarted.authorize_write(&WriteRequest::new(
                Caller::system(EntryPoint::ApiRequest, 21_010, "write"),
                class,
            ))),
            "authority_recovery_required"
        );
    }
    let health = restarted.health();
    assert_eq!(health.condition, "recovery_required");
    assert!(health.banner.contains("ledger_head="));
    assert!(health.banner.contains("anchor_head="));
    assert!(health.banner.contains("latch_token_invalid"));
    let row = restarted
        .audit()
        .into_iter()
        .rfind(|event| event.entry == EntryPoint::ApiRequest)
        .expect("refusal audit");
    assert!(row.detail.contains("ledger_head="));
    assert!(row.evidence.is_some());
    assert_ne!(health.condition, "dry_run_latched");
}

#[test]
fn s321_stored_token_evidence_cannot_reconstruct_the_bootstrap_capability() {
    let fixture = auto_fixture(22_000);
    let ledger = fixture.authority.ledger();
    let stored = ledger
        .latch()
        .map(|latch| latch.token().generation_text().to_owned())
        .expect("generation text");
    assert_eq!(stored, "1");
    assert_eq!(
        energy_decide::authority::BootstrapAuthorityTx::begin_external(
            ledger,
            22_010,
            "api",
            &fixture.anchors,
        )
        .expect_err("no capability from storage"),
        BootstrapTxError::ReplayDenied
    );
    let refusal = fixture
        .authority
        .reject_bootstrap_reconstruction(Caller::operator(
            EntryPoint::DashboardMutation,
            "api",
            22_020,
            "rebuild seal",
        ));
    assert_eq!(refusal, AuthorityRefusal::BootstrapTxCapabilityDenied);
}

#[test]
fn s322_previous_run_and_wrong_transaction_markers_are_refused() {
    let fixture = auto_fixture(23_000);
    let ledger = fixture.authority.ledger();
    let mut anchor = fixture.anchor().expect("anchor");
    let mut issue = BootstrapIssue::new(2, 99, 17, 23_000);
    issue.consumed = false;
    anchor.bootstrap_issue = Some(issue);
    fixture.restore_anchor(&anchor);
    assert_eq!(
        energy_decide::authority::BootstrapAuthorityTx::begin_external(
            ledger.clone(),
            23_010,
            "api",
            &fixture.anchors,
        )
        .expect_err("previous run"),
        BootstrapTxError::CapabilityDenied
    );

    let mut wrong_tx = BootstrapIssue::new(900, 1, 17, 23_000);
    wrong_tx.consumed = false;
    anchor.bootstrap_issue = Some(wrong_tx);
    fixture.restore_anchor(&anchor);
    assert_eq!(
        energy_decide::authority::BootstrapAuthorityTx::begin_external(
            ledger,
            23_020,
            "api",
            &fixture.anchors,
        )
        .expect_err("substituted transaction"),
        BootstrapTxError::CapabilityDenied
    );
}

#[test]
fn s323_from_scratch_replay_uses_all_durable_state_and_stays_denied() {
    let fixture = auto_fixture(24_000);
    let snapshot = fixture.authority.ledger();
    let anchor = fixture.anchor().expect("anchor");
    let records = fixture.records();
    assert_ne!(records.len(), 0);
    assert!(anchor.bootstrap_issue.is_some());
    let replay = energy_decide::authority::BootstrapAuthorityTx::begin_external(
        snapshot,
        24_010,
        "api",
        &fixture.anchors,
    );
    assert_eq!(replay.expect_err("replay"), BootstrapTxError::ReplayDenied);
}

#[test]
fn s324_reconcile_permit_is_required_at_the_dispatcher_boundary() {
    let io = m4_support::FakeIo::new(&[("P", m4_support::Behavior::Accept { echo: true })]);
    let dut = Dispatcher::new_with_shared(
        Arc::new(io),
        ProceedSafety,
        AllowAllAuthority,
        m4_support::ImmediateClock::default(),
        1_000,
    );
    let mut state = DispatchState::default();
    let permit = m4_support::reconcile_permit();
    assert_eq!(dut.reconcile(&mut state, 25_000, &permit), 0);

    let fixture = auto_fixture(25_000);
    let ordinary = Caller::operator(EntryPoint::InternalRecovery, "api", 25_010, "reconcile");
    assert_eq!(
        fixture
            .authority
            .authorize_reconcile(&ordinary)
            .expect_err("ordinary caller"),
        AuthorityRefusal::ReconcileDenied
    );
}

#[test]
fn s325_each_named_entry_point_is_refused_while_latched_and_proceeds_in_auto() {
    let cases: Vec<EntryCase> = entry_point_cases();
    let latched = m6_support::latched_fixture(26_000);
    let auto = auto_fixture(26_000);
    for (name, exercise) in cases {
        let denied = exercise(&latched.authority);
        assert!(!denied, "{name} must be refused while latched");
        let allowed = exercise(&auto.authority);
        assert!(allowed, "{name} must proceed in auto");
    }
}

fn decision_allowed(decision: AuthorityDecision) -> bool {
    matches!(decision, AuthorityDecision::Authorized)
}

type EntryCase = (&'static str, Box<dyn Fn(&Authority) -> bool>);

fn entry_point_cases() -> Vec<EntryCase> {
    let mut cases: Vec<EntryCase> = vec![
        (
            "engine dispatch",
            Box::new(|authority| {
                decision_allowed(authority.authorize(&command("P", -800.0, 26_000)))
            }),
        ),
        (
            "safety traffic",
            Box::new(|authority| {
                let mut held = fenced_hold("P", "fence-26", 26_000);
                held.safety_traffic = true;
                decision_allowed(authority.authorize(&held))
            }),
        ),
        (
            "boot neutralisation",
            Box::new(|authority| {
                authority
                    .authorize_write(&WriteRequest::new(
                        Caller::system(EntryPoint::Startup, 26_000, "boot neutralisation"),
                        WriteClass::DeviceNeutralisation {
                            fence_id: "fence-boot".to_owned(),
                        },
                    ))
                    .authorized()
            }),
        ),
    ];
    cases.extend(neutralisation_and_passthrough_cases());
    cases
}

fn neutralisation_and_passthrough_cases() -> Vec<EntryCase> {
    let cases: Vec<EntryCase> = vec![
        (
            "shutdown neutralisation",
            Box::new(|authority| {
                authority
                    .authorize_write(&WriteRequest::new(
                        Caller::system(
                            EntryPoint::WatchdogRestart,
                            26_000,
                            "shutdown neutralisation",
                        ),
                        WriteClass::DeviceNeutralisation {
                            fence_id: "fence-shutdown".to_owned(),
                        },
                    ))
                    .authorized()
            }),
        ),
        (
            "settings mode control plane",
            Box::new(|authority| {
                authority
                    .authorize_persistence(
                        Caller::system(EntryPoint::DashboardMutation, 26_000, "settings"),
                        WriteClass::DomainPersistence,
                    )
                    .authorized()
            }),
        ),
        (
            "generic websocket passthrough",
            Box::new(|authority| {
                authority
                    .authorize_ws_passthrough(
                        Caller::system(EntryPoint::GenericWsPassthrough, 26_000, "raw ws call"),
                        WriteClass::DeviceEconomic,
                    )
                    .authorized()
            }),
        ),
    ];
    cases
}

#[test]
fn s326_anchor_detects_older_foreign_and_database_only_replacements() {
    let fixture = auto_fixture(27_000);
    let older = fixture.authority.ledger();
    let snapshot = fixture.anchor().expect("anchor");

    fixture
        .authority
        .begin_clearance(&ClearanceCommand::new("operator", 27_010, "c-27", true))
        .expect("advanced ledger");
    let mut advanced = fixture.anchor().expect("anchor");
    advanced.committed_head = fixture.authority.ledger().head_event_id();
    advanced.journal_hash_chain_head = fixture.authority.ledger().chain_head();
    advanced.checkpoint_event_id = fixture.authority.ledger().checkpoint_event_id();
    fixture.restore_anchor(&advanced);

    let older_db = fixture.restart(older.clone(), 27_020);
    assert_eq!(
        older_db.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert_eq!(older_db.startup().reason, Some("anchor_mismatch"));

    let foreign = auto_fixture(27_100);
    let foreign_db = foreign.restart(fixture.authority.ledger(), 27_030);
    assert_eq!(
        foreign_db.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert_eq!(foreign_db.startup().reason, Some("anchor_mismatch"));

    let coherent = auto_fixture(27_040);
    coherent.restore_anchor(&snapshot);
    let restored = coherent.restart(older, 27_050);
    assert_eq!(
        restored.startup().resolution,
        StartupResolution::Reconciled,
        "coherent both-file rollback is locally indistinguishable and documented"
    );
}

#[test]
fn s327_evidence_is_part_of_the_recovery_api_not_only_a_log() {
    let fixture = auto_fixture(28_000);
    let mut ledger = fixture.authority.ledger();
    ledger.set_checkpoint(0);
    let restarted = fixture.restart(ledger, 28_010);
    let health = restarted.health();
    let evidence: RecoveryEvidence = health.recovery.clone().expect("structured evidence");
    assert_eq!(evidence.reason, "journal_incomplete");
    assert_eq!(evidence.ledger_checkpoint, 0);
    assert!(health.banner.contains("journal_incomplete"));
    assert!(health.code_string().contains("authority_recovery_required"));
}
