//! M6 single-gate scenarios: S145, S156, S172, S185, S186, S196, S197, S200, S206, S230.
//! Production lines caught by these regressions:
//! `gate::decide` (no device write in latched/clearing, no passthrough bypass);
//! `Authority::decide` (audit on every refusal, no handler-local flags);
//! `authority_ops::execute_intent` (execution-time re-check, typed refusal);
//! `impl WriteAuthority for Authority` (M4 row must show the gate's reason).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod m6_support;

use energy_decide::authority::{
    AuthorityRefusal, AuthorityState, Caller, ClearanceCommand, EntryPoint, GateOutcome,
    QueuedIntent, WriteClass, WriteRequest,
};
use energy_decide::dispatch::{
    AuthorityDecision, DispatchOutcome, DispatchState, SlotRequest, WriteAuthority,
};
use m6_support::{auto_fixture, command, fenced_hold, latched_fixture};

fn reason(outcome: &GateOutcome) -> String {
    match outcome {
        GateOutcome::Authorized => "authorized".to_owned(),
        GateOutcome::Refused(refusal) => refusal.slug().to_owned(),
    }
}

#[test]
fn s145_dry_run_latched_writes_nothing_even_for_faults_and_holds() {
    let fixture = latched_fixture(1_000);
    let authority = &fixture.authority;
    let before = authority.gate_calls();
    for index in 0..1_000 {
        let mut cmd = command("P1", -800.0, 1_000 + index);
        if index % 3 == 0 {
            cmd.safety_traffic = true;
            cmd.fence_id = Some(format!("cell-{index}"));
        }
        assert_eq!(
            authority.authorize(&cmd),
            AuthorityDecision::Denied {
                reason: "write_denied_by_authority_gate"
            }
        );
    }
    assert_eq!(authority.state(), AuthorityState::DryRunLatched);
    assert_eq!(authority.gate_calls(), before + 1_000);
    assert_eq!(
        authority
            .ledger()
            .latch()
            .map(energy_decide::authority::Latch::state),
        Some(AuthorityState::DryRunLatched)
    );
    assert!(
        authority
            .audit()
            .iter()
            .all(|event| !event.outcome.authorized()
                || event.entry == EntryPoint::StartupResolutionWrite)
    );
    assert!(!authority.health().safety_active);
    assert_eq!(
        authority.health().code_string(),
        "protection=none,battery_unmanaged=true,warning=no_protection_writes_forbidden"
    );
}

#[test]
fn s156_no_device_write_is_authorized_while_latched() {
    let fixture = latched_fixture(2_000);
    let mut emergency = command("P", 800.0, 2_000);
    emergency.safety_traffic = true;
    let outcome = fixture.authority.authorize_write(&WriteRequest::new(
        Caller::system(EntryPoint::TickDispatch, 2_000, "cell latch"),
        energy_decide::authority::command_class(&emergency),
    ));
    assert!(!outcome.authorized());
    m6_support::assert_only_three_states();
}

#[test]
fn s172_mode_endpoint_cannot_clear_the_latch() {
    let fixture = latched_fixture(3_000);
    let caller = Caller::operator(EntryPoint::ModeEndpoint, "operator", 3_000, "set auto");
    let outcome = fixture.authority.authorize_write(&WriteRequest::new(
        caller,
        WriteClass::ControlPlaneAuthority,
    ));
    assert_eq!(reason(&outcome), "authority_transition_denied");
    assert_eq!(fixture.authority.state(), AuthorityState::DryRunLatched);
    assert!(
        fixture.authority.audit().iter().any(
            |event| event.detail == "mode_migration" || event.entry == EntryPoint::ModeEndpoint
        )
    );
}

#[test]
fn s185_generic_ws_passthrough_cannot_bypass_the_gate() {
    let fixture = latched_fixture(4_000);
    let caller = Caller::operator(EntryPoint::GenericWsPassthrough, "api", 4_000, "raw call");
    let light = fixture
        .authority
        .authorize_ws_passthrough(caller.clone(), WriteClass::DeviceEconomic);
    let settings = fixture
        .authority
        .authorize_ws_passthrough(caller, WriteClass::ControlPlaneAuthority);
    assert_eq!(reason(&light), "write_denied_by_authority_gate");
    assert_eq!(reason(&settings), "authority_transition_denied");
    assert!(
        fixture
            .authority
            .ledger()
            .events()
            .iter()
            .any(|event| matches!(
                event.kind,
                energy_decide::authority::AuthorityEventKind::DryRunLatchPreserved { ref path }
                    if path == "generic_ws_passthrough"
            ))
    );
}

#[test]
fn s186_manual_token_edit_drops_to_recovery_and_refuses_authoritative_writes() {
    let fixture = auto_fixture(5_000);
    let mut ledger = fixture.authority.ledger();
    ledger.corrupt_generation("1x");
    let restarted = fixture.restart(ledger, 5_100);
    assert_eq!(
        restarted.phase(),
        energy_decide::authority::AuthorityPhase::Invalid
    );
    assert_eq!(
        restarted.resolution(),
        energy_decide::authority::StartupResolution::RecoveryRequired
    );
    assert_eq!(
        reason(&restarted.authorize_write(&WriteRequest::new(
            Caller::system(EntryPoint::TickDispatch, 5_100, "economic"),
            WriteClass::DeviceEconomic,
        ))),
        "authority_recovery_required"
    );
    assert_eq!(
        restarted.health().text,
        energy_decide::authority::RECOVERY_TEXT
    );
}

#[test]
fn s196_write_classes_are_disjoint_and_persistence_is_state_scoped() {
    let fixture = auto_fixture(6_000);
    let journal = Caller::system(EntryPoint::Schedule, 6_000, "journal");
    assert!(
        fixture
            .authority
            .authorize_persistence(journal.clone(), WriteClass::ControlPlaneJournal)
            .authorized()
    );
    assert!(
        fixture
            .authority
            .authorize_persistence(journal.clone(), WriteClass::DomainPersistence)
            .authorized()
    );
    let permit = fixture
        .authority
        .begin_clearance(&ClearanceCommand::new("operator", 6_010, "clear-1", true))
        .expect("logged clearance");
    assert_clearing_classes(&fixture, journal, &permit);
}

fn assert_clearing_classes(
    fixture: &m6_support::Fixture,
    journal: Caller,
    permit: &energy_decide::authority::ClearancePermit,
) {
    assert_eq!(
        reason(
            &fixture
                .authority
                .authorize_persistence(journal.clone(), WriteClass::DomainPersistence)
        ),
        "persistence_denied"
    );
    assert_eq!(
        reason(
            &fixture
                .authority
                .authorize_persistence(journal.clone(), WriteClass::DeviceEconomic)
        ),
        "write_denied_by_authority_gate"
    );
    assert!(
        fixture
            .authority
            .authorize_persistence(journal, WriteClass::ControlPlaneJournal)
            .authorized()
    );
    let fresh_fence = fenced_hold("P", "fence-1", 6_020);
    assert_eq!(
        fixture.authority.authorize(&fresh_fence),
        AuthorityDecision::Authorized
    );
    assert_eq!(
        reason(&fixture.authority.authorize_write(&WriteRequest::new(
            Caller::system(EntryPoint::Schedule, 6_021, "replayed fence"),
            WriteClass::DeviceNeutralisation {
                fence_id: "fence-1".to_owned()
            },
        ))),
        "fresh_fence_required"
    );
    fixture
        .authority
        .commit_clearance(permit, 6_030)
        .expect("clearance commit");
}

#[test]
fn s197_safety_precedence_is_scoped_by_authority() {
    let fixture = latched_fixture(7_000);
    let mut emergency = command("P", 800.0, 7_000);
    emergency.safety_traffic = true;
    assert!(matches!(
        fixture.authority.authorize(&emergency),
        AuthorityDecision::Denied { .. }
    ));

    let auto = auto_fixture(7_100);
    assert!(matches!(
        auto.authority.authorize(&emergency),
        AuthorityDecision::Authorized
    ));
}

#[test]
fn s200_queued_intent_is_rechecked_and_refusal_is_recorded() {
    let fixture = auto_fixture(8_000);
    let intent = QueuedIntent::new(42, command("P", -800.0, 8_000), 8_000, AuthorityState::Auto);
    assert!(
        fixture
            .authority
            .execute_intent(intent.clone())
            .authorized()
    );
    let permit = fixture
        .authority
        .begin_clearance(&ClearanceCommand::new("operator", 8_010, "clear-42", true))
        .expect("logged clearance");
    fixture
        .authority
        .fail_clearance(&permit, "clearance_timeout", 8_020)
        .expect("fail");
    let outcome = fixture.authority.execute_intent(intent);
    match outcome {
        energy_decide::authority::IntentOutcome::Refused {
            intent_id,
            reason,
            entry,
        } => {
            assert_eq!(intent_id, 42);
            assert_eq!(reason, AuthorityRefusal::IntentNotAuthorized);
            assert_eq!(entry, EntryPoint::QueuedIntentExecution);
        }
        energy_decide::authority::IntentOutcome::Authorized { .. } => {
            panic!("expected typed refusal")
        }
    }
    assert!(
        fixture
            .authority
            .audit()
            .iter()
            .any(|event| event.refused() && event.entry == EntryPoint::QueuedIntentExecution)
    );
}

#[test]
fn s230_m4_dispatcher_passes_through_the_same_gate() {
    #[path = "m4_support/mod.rs"]
    mod m4_support;
    use energy_decide::dispatch::ProceedSafety;
    use m4_support::{FakeIo, ImmediateClock};

    let fixture = latched_fixture(9_000);
    let io = FakeIo::default();
    let dispatcher = energy_decide::dispatch::Dispatcher::new(
        io.clone(),
        ProceedSafety,
        fixture.authority,
        ImmediateClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let request = SlotRequest::economic("P", -800.0, 9_000, "plan");
    let rows = dispatcher.dispatch_slot(&mut state, std::slice::from_ref(&request), 9_000);
    assert_eq!(rows[0].outcome, DispatchOutcome::Skipped);
    assert!(!rows[0].written);
    assert_eq!(io.write_count(), 0);
    assert_eq!(
        rows[0].reasons,
        vec!["authority_denied", "write_denied_by_authority_gate"]
    );
}

#[test]
fn s206_reconcile_is_authority_only() {
    let fixture = auto_fixture(10_000);
    let ordinary = Caller::operator(EntryPoint::InternalRecovery, "api", 10_000, "reconcile");
    assert!(matches!(
        fixture.authority.authorize_reconcile(&ordinary),
        Err(AuthorityRefusal::ReconcileDenied)
    ));
    assert_eq!(
        reason(&fixture.authority.authorize_write(&WriteRequest::new(
            ordinary,
            WriteClass::ControlPlaneAuthority,
        ))),
        "reconcile_denied"
    );
}

#[test]
fn s102_operational_dry_run_request_is_rejected() {
    let fixture = auto_fixture(11_000);
    let caller = Caller::operator(EntryPoint::ApiRequest, "api", 11_000, "set dry_run");
    assert_eq!(
        fixture.authority.decline_operational_dry_run(caller),
        AuthorityRefusal::DryRunNotOperational
    );
    assert_eq!(fixture.authority.state(), AuthorityState::Auto);
}

#[test]
fn s313_every_entry_point_is_counted_by_the_one_gate() {
    let fixture = auto_fixture(13_000);
    let entries = [
        EntryPoint::ModeEndpoint,
        EntryPoint::LegacyAlias,
        EntryPoint::AuditExpiry,
        EntryPoint::ConfigReload,
        EntryPoint::ApiRequest,
        EntryPoint::GenericWsPassthrough,
        EntryPoint::Schedule,
        EntryPoint::WatchdogRestart,
        EntryPoint::TickDispatch,
        EntryPoint::MaintenancePersistence,
        EntryPoint::LearningPersistence,
        EntryPoint::ManualStateEdit,
        EntryPoint::DashboardMutation,
        EntryPoint::LedgerWrite,
        EntryPoint::AnchorUpdate,
        EntryPoint::StartupResolutionWrite,
    ];
    let before = fixture.authority.gate_calls();
    for (index, entry) in entries.iter().enumerate() {
        let class = if *entry == EntryPoint::StartupResolutionWrite {
            WriteClass::StartupResolution
        } else {
            WriteClass::ControlPlaneJournal
        };
        let outcome = fixture.authority.authorize_write(&WriteRequest::new(
            Caller::system(
                *entry,
                13_000 + i64::try_from(index).unwrap_or(0),
                "counted",
            ),
            class,
        ));
        assert!(outcome.authorized());
        assert_eq!(
            fixture.authority.gate_calls(),
            before + u64::try_from(index).unwrap_or(0) + 1
        );
    }
    let audit = fixture.authority.audit();
    assert_eq!(audit.len() as u64, fixture.authority.gate_calls());
    for event in &audit {
        assert_ne!(event.actor.as_str(), "");
        assert_ne!(event.why.as_str(), "");
        assert!(event.at_ms > 0);
    }
}
