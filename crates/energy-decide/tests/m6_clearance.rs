//! M6 clearance, latch-preservation and health-text scenarios: S171-S178, S183, S187, S195, S201, S207.
//! Production lines caught by these regressions:
//! `authority_ops::{begin_clearance,commit_clearance,fail_clearance}` (state before write,
//! fence freshness, journal ordering, no partial clearance) and `health::report`
//! (identical human-readable text in dashboard and API).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod m6_support;

use energy_decide::authority::{
    ACTIVE_TEXT, AuthorityEventKind, AuthorityState, CLEARING_TEXT, Caller, ClearanceCommand,
    EntryPoint, LATCHED_TEXT, RECOVERY_TEXT, StartupResolution, WriteClass, WriteRequest,
};
use energy_decide::dispatch::WriteAuthority;
use m6_support::{auto_fixture, command, fenced_hold, latched_fixture};

#[test]
fn s178_clearance_enters_first_and_allows_only_fenced_neutralisation() {
    let fixture = auto_fixture(1_000);
    let permit = fixture
        .authority
        .begin_clearance(&ClearanceCommand::new("operator", 1_010, "clear-7", true))
        .expect("logged clearance");
    assert_eq!(fixture.authority.state(), AuthorityState::Clearing);
    assert_eq!(permit.barrier_id(), "clear-7:2");
    assert_eq!(
        fixture.authority.authorize(&command("P", -800.0, 1_020)),
        energy_decide::dispatch::AuthorityDecision::Denied {
            reason: "write_denied_by_authority_gate"
        }
    );
    assert_eq!(
        fixture
            .authority
            .authorize(&fenced_hold("P", "fence-7", 1_025)),
        energy_decide::dispatch::AuthorityDecision::Authorized
    );
    fixture
        .authority
        .commit_clearance(&permit, 1_100)
        .expect("clearance");
    assert_eq!(fixture.authority.state(), AuthorityState::Auto);
    assert_eq!(
        fixture.authority.authorize(&command("P", -800.0, 1_030)),
        energy_decide::dispatch::AuthorityDecision::Authorized
    );
}

#[test]
fn s183_timeout_returns_to_latched_with_no_partial_clearance() {
    let fixture = auto_fixture(2_000);
    let permit = fixture
        .authority
        .begin_clearance(&ClearanceCommand::new("operator", 2_010, "clear-9", true))
        .expect("logged clearance");
    fixture
        .authority
        .fail_clearance(&permit, "clearance_timeout", 2_120)
        .expect("timeout");
    assert_eq!(fixture.authority.state(), AuthorityState::DryRunLatched);
    assert!(fixture.authority.ledger().events().iter().any(|event| matches!(
        event.kind,
        AuthorityEventKind::AuthorityTransition { ref reason, .. } if reason == "clearance_timeout"
    )));
    assert_eq!(
        fixture.authority.authorize(&command("P", -800.0, 2_130)),
        energy_decide::dispatch::AuthorityDecision::Denied {
            reason: "write_denied_by_authority_gate"
        }
    );
}

#[test]
fn s187_barrier_identity_and_fence_freshness_are_enforced() {
    let fixture = auto_fixture(3_000);
    let permit = fixture
        .authority
        .begin_clearance(&ClearanceCommand::new("operator", 3_010, "clear-11", true))
        .expect("logged clearance");
    assert_eq!(
        permit.barrier_id(),
        format!("clear-11:{}", permit.generation())
    );
    let hold = fenced_hold("P", "fence-11", 3_020);
    assert_eq!(
        fixture.authority.authorize(&hold),
        energy_decide::dispatch::AuthorityDecision::Authorized
    );
    assert_eq!(
        fixture.authority.authorize(&hold),
        energy_decide::dispatch::AuthorityDecision::Denied {
            reason: "fresh_fence_required"
        }
    );
    let invalid = ClearanceCommand::new("", 3_030, "clear-12", true);
    assert!(fixture.authority.begin_clearance(&invalid).is_err());
    assert_eq!(fixture.authority.state(), AuthorityState::Clearing);
}

#[test]
fn s195_journal_barrier_commit_order_is_auditable() {
    let fixture = auto_fixture(4_000);
    let permit = fixture
        .authority
        .begin_clearance(&ClearanceCommand::new("operator", 4_010, "clear-13", true))
        .expect("entry");
    let _ = fixture
        .authority
        .authorize(&fenced_hold("P", "fence-13", 4_020));
    fixture
        .authority
        .commit_clearance(&permit, 4_050)
        .expect("commit");
    let events = fixture.authority.ledger().events().to_vec();
    let families = events
        .iter()
        .map(|event| event.kind.family())
        .collect::<Vec<_>>();
    let entry = families
        .iter()
        .position(|family| *family == "clearance_entry")
        .expect("entry event");
    let transition = families
        .iter()
        .position(|family| *family == "authority_transition")
        .expect("transition event");
    assert!(entry < transition);
    assert!(events[entry].at_ms < events[transition].at_ms);
    assert!(matches!(
        events[transition].kind,
        AuthorityEventKind::AuthorityTransition {
            to: AuthorityState::Auto,
            clearance_id: Some(_),
            ..
        }
    ));
}

#[test]
fn s171_to_s177_every_latch_preservation_path_reaches_the_gate() {
    let fixture = latched_fixture(5_000);
    for entry in [
        EntryPoint::Startup,
        EntryPoint::ModeEndpoint,
        EntryPoint::LegacyAlias,
        EntryPoint::AuditExpiry,
        EntryPoint::ConfigReload,
        EntryPoint::ApiRequest,
        EntryPoint::Schedule,
        EntryPoint::WatchdogRestart,
        EntryPoint::ManualStateEdit,
    ] {
        let caller = Caller::system(entry, 5_000, "lifecycle path");
        let outcome = fixture
            .authority
            .authorize_write(&WriteRequest::new(caller, WriteClass::DeviceEconomic));
        assert!(!outcome.authorized());
        assert!(
            fixture
                .authority
                .ledger()
                .events()
                .iter()
                .any(|event| matches!(
                    event.kind,
                    AuthorityEventKind::DryRunLatchPreserved { ref path } if path == entry.as_str()
                ))
        );
    }
    assert_eq!(fixture.authority.state(), AuthorityState::DryRunLatched);
}

#[test]
fn s201_and_s207_health_text_is_identical_in_dashboard_and_api_shapes() {
    let latched = latched_fixture(6_000);
    assert_eq!(latched.authority.health().text, LATCHED_TEXT);
    assert_eq!(
        latched.authority.health().code_string(),
        "protection=none,battery_unmanaged=true,warning=no_protection_writes_forbidden"
    );
    assert!(!latched.authority.health().safety_active);

    let fixture = auto_fixture(6_010);
    let permit = fixture
        .authority
        .begin_clearance(&ClearanceCommand::new("operator", 6_020, "clear-15", true))
        .expect("clearance");
    assert_eq!(fixture.authority.health().text, CLEARING_TEXT);
    assert_eq!(
        fixture.authority.health().code_string(),
        "protection=neutralisation_only,battery_unmanaged=true,warning=clearance_in_progress"
    );
    fixture
        .authority
        .commit_clearance(&permit, 6_050)
        .expect("commit");
    assert_eq!(fixture.authority.health().text, ACTIVE_TEXT);
    assert!(fixture.authority.health().safety_active);

    let mut ledger = fixture.authority.ledger();
    ledger.remove_latch();
    let recovery = fixture.restart(ledger, 6_030);
    assert_eq!(recovery.resolution(), StartupResolution::RecoveryRequired);
    assert_eq!(recovery.health().text, RECOVERY_TEXT);
}
