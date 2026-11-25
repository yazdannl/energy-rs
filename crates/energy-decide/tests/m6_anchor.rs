//! M6 anchor, checkpoint, startup-resolution and durability scenarios: S208-S220.
//! Production lines caught by these regressions:
//! `FileAnchorStore`/`atomic_file::replace` (torn file and directory fsync);
//! `startup_steps::{precheck,install_ref}` (database-only rollback, stale checkpoint,
//! pending-head crash boundaries, resolution-record durability).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod m6_support;

use energy_decide::authority::{
    AuthorityLedger, AuthorityPhase, AuthorityRefusal, AuthorityState, CONTINUITY_BANNER, Caller,
    ClearanceCommand, EntryPoint, GateOutcome, ResolutionStore, StartupResolution,
    StartupResolutionRecord, StorageError, WriteClass, WriteRequest,
};
use m6_support::{Fixture, TestDir, auto_fixture, latched_fixture};

fn device_is_denied(authority: &energy_decide::authority::Authority, at_ms: i64) -> String {
    match authority.authorize_write(&WriteRequest::new(
        Caller::system(EntryPoint::TickDispatch, at_ms, "economic"),
        WriteClass::DeviceEconomic,
    )) {
        GateOutcome::Authorized => "authorized".to_owned(),
        GateOutcome::Refused(reason) => reason.slug().to_owned(),
    }
}

#[test]
fn s208_legacy_evidence_snapshot_rollback_mismatch_and_truncation() {
    let mut legacy_ledger = AuthorityLedger::new(1, Some("auto"), "0.14.9");
    legacy_ledger.seed_journal_note(1_000, "pre-authority");
    let legacy = Fixture::new(legacy_ledger, 1_000);
    assert_eq!(
        legacy.authority.startup().resolution,
        StartupResolution::Reconciled
    );

    let auto = auto_fixture(2_000);
    let snapshot = auto.authority.ledger();
    auto.authority
        .begin_clearance(&ClearanceCommand::new("operator", 2_010, "c-1", true))
        .expect("logged clearance");

    let coherent = auto.restart(snapshot.clone(), 2_020);
    assert_eq!(coherent.startup().resolution, StartupResolution::Reconciled);
    assert!(
        coherent
            .startup()
            .health
            .banner
            .starts_with(CONTINUITY_BANNER)
    );
    assert!(
        auto.records()
            .iter()
            .any(|record| record.case == StartupResolution::Reconciled)
    );

    assert_database_attacks(&auto, snapshot);
}

fn assert_database_attacks(auto: &Fixture, snapshot: AuthorityLedger) {
    let mut advanced_anchor = auto.anchor().expect("anchor");
    advanced_anchor.committed_head = 2;
    auto.restore_anchor(&advanced_anchor);
    let db_only = auto.restart(snapshot, 2_030);
    assert_eq!(
        db_only.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert_eq!(db_only.startup().reason, Some("anchor_mismatch"));
    assert!(
        db_only
            .startup()
            .health
            .banner
            .starts_with(CONTINUITY_BANNER)
    );
    assert!(db_only.startup().health.banner.contains("anchor_mismatch"));

    let mut truncated = auto.authority.ledger();
    truncated.truncate_tail(1);
    let torn = auto.restart(truncated, 2_040);
    assert_eq!(
        torn.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert_eq!(
        torn.startup().reason,
        Some("recovery_required_unreconciled"),
        "the earlier database-only rollback locked recovery for this install"
    );
}

#[test]
fn s209_checkpoint_ownership_and_completeness() {
    let auto = auto_fixture(3_000);
    let mut stale = auto.authority.ledger();
    stale.set_checkpoint(0);
    let restarted = auto.restart(stale, 3_010);
    assert_eq!(restarted.startup().reason, Some("journal_incomplete"));

    let checkpoint_attempt = auto.authority.authorize_write(&WriteRequest::new(
        Caller::system(EntryPoint::ApiRequest, 3_020, "ordinary append"),
        WriteClass::CheckpointWrite,
    ));
    assert_eq!(
        checkpoint_attempt,
        GateOutcome::Refused(AuthorityRefusal::CheckpointWriteDenied)
    );
}

fn reconcile_after_repair(fixture: &Fixture, authority: &energy_decide::authority::Authority) {
    let evidence = authority.startup().evidence.expect("locked evidence");
    let permit = authority
        .issue_reconcile_permit("operator", 4_025, "repaired ledger")
        .expect("authority permit");
    authority
        .reconcile_recovery(
            &fixture.anchors,
            &fixture.resolutions,
            permit,
            &evidence,
            4_030,
        )
        .expect("reconcile");
}

#[test]
fn s211_only_two_resolutions_and_a_banner_on_every_startup() {
    let fixture = auto_fixture(4_000);
    let clean = fixture.authority.startup();
    assert_eq!(clean.resolution, StartupResolution::Reconciled);
    assert_eq!(clean.health.banner, CONTINUITY_BANNER);

    let mut ledger = fixture.authority.ledger();
    ledger.set_checkpoint(0);
    let crashed = fixture.restart(ledger, 4_010);
    assert_eq!(
        crashed.startup().resolution,
        StartupResolution::RecoveryRequired
    );

    let snapshot = fixture.authority.ledger();
    let restored = fixture.restart(snapshot, 4_020);
    assert_eq!(
        restored.startup().resolution,
        StartupResolution::RecoveryRequired,
        "repairing the ledger alone cannot clear recovery"
    );
    reconcile_after_repair(&fixture, &restored);
    assert_eq!(restored.startup().resolution, StartupResolution::Reconciled);
    assert_eq!(restored.startup().health.banner, CONTINUITY_BANNER);

    let no_anchor = {
        let dir = TestDir::new();
        let anchors = energy_decide::authority::FileAnchorStore::in_dir(&dir.path);
        let resolutions = energy_decide::authority::FileResolutionStore::in_dir(&dir.path);
        energy_decide::authority::Authority::start(
            fixture.authority.ledger(),
            &anchors,
            &resolutions,
            4_030,
        )
    };
    assert_eq!(
        no_anchor.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert!(
        no_anchor
            .startup()
            .health
            .banner
            .starts_with(CONTINUITY_BANNER)
    );
    assert!(
        no_anchor
            .startup()
            .health
            .banner
            .contains("anchor_mismatch")
    );
    assert_ne!(no_anchor.startup().resolution.as_str(), "continuity_proven");
}

#[test]
fn s212_restore_never_changes_authority() {
    let latched = latched_fixture(5_000);
    let snapshot = latched.authority.ledger();
    let anchor = latched.anchor().expect("anchor");
    latched.restore_anchor(&anchor);
    let restored = latched.restart(snapshot, 5_010);
    assert_eq!(restored.state(), AuthorityState::DryRunLatched);
    assert!(!restored.startup().dispatch_eligible);
    assert_eq!(
        device_is_denied(&restored, 5_010),
        "write_denied_by_authority_gate"
    );

    let auto = auto_fixture(5_020);
    assert!(auto.authority.startup().dispatch_eligible);
    assert_eq!(auto.authority.phase(), AuthorityPhase::Initialized);
}

#[test]
fn s213_prepare_abort_discards_pending_head() {
    let fixture = auto_fixture(6_000);
    let ledger = fixture.authority.ledger();
    let mut anchor = fixture.anchor().expect("anchor");
    anchor.pending_head = 2;
    fixture.restore_anchor(&anchor);
    let restarted = fixture.restart(ledger, 6_010);
    assert_eq!(
        restarted.startup().resolution,
        StartupResolution::Reconciled
    );
    let promoted = fixture.anchor().expect("anchor");
    assert_eq!(promoted.pending_head, 0);
    assert_eq!(promoted.committed_head, 1);
}

#[test]
fn s214_committed_event_with_pending_head_is_adopted() {
    let fixture = auto_fixture(7_000);
    fixture
        .authority
        .begin_clearance(&ClearanceCommand::new("operator", 7_010, "c-7", true))
        .expect("logged clearance");
    let ledger = fixture.authority.ledger();
    let mut anchor = fixture.anchor().expect("anchor");
    anchor.pending_head = ledger.head_event_id();
    anchor.checkpoint_event_id = ledger.checkpoint_event_id();
    anchor.journal_hash_chain_head = ledger.chain_head();
    fixture.restore_anchor(&anchor);
    let restarted = fixture.restart(ledger, 7_020);
    assert_eq!(
        restarted.startup().resolution,
        StartupResolution::Reconciled
    );
    let promoted = fixture.anchor().expect("anchor");
    assert_eq!(promoted.committed_head, 2);
    assert_eq!(promoted.pending_head, 0);
}

#[test]
fn s215_atomic_anchor_replacement_never_exposes_a_torn_file() {
    let fixture = auto_fixture(8_000);
    let mut anchor = fixture.anchor().expect("anchor");
    for head in 2..6 {
        anchor.committed_head = head;
        fixture.restore_anchor(&anchor);
        let loaded = fixture.anchor().expect("complete anchor");
        assert_eq!(loaded.committed_head, head);
        let leftovers = std::fs::read_dir(&fixture.dir.path)
            .expect("dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("tmp-"))
            .count();
        assert_eq!(leftovers, 0);
    }
    let restarted = fixture.restart(fixture.authority.ledger(), 8_010);
    assert_eq!(
        restarted.startup().resolution,
        StartupResolution::RecoveryRequired
    );
}

#[test]
fn s216_corrupt_anchor_fails_closed_without_bootstrap() {
    let fixture = auto_fixture(9_000);
    std::fs::write(fixture.anchors.path(), "{not-json").expect("corrupt anchor");
    let restarted = fixture.restart(fixture.authority.ledger(), 9_010);
    assert_eq!(
        restarted.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert_eq!(
        restarted.startup().reason,
        Some("resolution_record_corrupt")
    );
    assert_eq!(
        device_is_denied(&restarted, 9_010),
        "authority_recovery_required"
    );
}

#[test]
fn s217_startup_record_is_writable_with_invalid_authority() {
    let fixture = auto_fixture(10_000);
    let mut ledger = fixture.authority.ledger();
    ledger.remove_latch();
    ledger.truncate_tail(0);
    let restarted = fixture.restart(ledger, 10_010);
    assert_eq!(
        restarted.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert_eq!(restarted.startup().phase, AuthorityPhase::Invalid);
    assert!(
        fixture
            .records()
            .iter()
            .any(|record| record.case == StartupResolution::RecoveryRequired)
    );

    std::fs::write(fixture.resolutions.path(), "").expect("delete referenced record");
    let orphan = fixture.restart(fixture.authority.ledger(), 10_020);
    assert_eq!(
        orphan.startup().resolution,
        StartupResolution::RecoveryRequired
    );
    assert_eq!(
        orphan.startup().reason,
        Some("recovery_required_unreconciled"),
        "the durable lock survives even evidence destruction"
    );
}

#[test]
fn s219_first_bootstrap_anchor_includes_the_resolution_ref() {
    let fixture = auto_fixture(11_000);
    let anchor = fixture.anchor().expect("anchor");
    let records = fixture.records();
    assert_eq!(
        anchor.last_startup_resolution_ref,
        records.first().map(|record| record.id)
    );
    assert_eq!(anchor.pending_startup_ref, None);

    let later = fixture.restart(fixture.authority.ledger(), 11_010);
    assert_eq!(later.startup().resolution, StartupResolution::Reconciled);
    assert_eq!(fixture.records().len(), 2);
    let anchor = fixture.anchor().expect("anchor");
    assert_eq!(anchor.last_startup_resolution_ref, Some(2));
}

#[test]
fn s220_persistence_failures_are_typed_and_write_free() {
    for error in [
        StorageError::StorageFull,
        StorageError::ReadOnly,
        StorageError::FsyncFailed,
        StorageError::DirFsyncFailed,
        StorageError::Corrupt,
        StorageError::PendingRefFailed,
        StorageError::PromotionFailed,
    ] {
        let dir = TestDir::new();
        let anchors = energy_decide::authority::FileAnchorStore::in_dir(&dir.path);
        let resolutions = FailingResolutionStore { error };
        let authority = energy_decide::authority::Authority::start(
            auto_fixture(12_000).authority.ledger(),
            &anchors,
            &resolutions,
            12_000,
        );
        assert_eq!(
            authority.startup().resolution,
            StartupResolution::RecoveryRequired
        );
        assert_eq!(authority.startup().reason, Some(error.slug()));
        assert_eq!(
            device_is_denied(&authority, 12_000),
            "authority_recovery_required"
        );
        assert!(
            authority
                .startup()
                .health
                .banner
                .starts_with(CONTINUITY_BANNER)
        );
    }
}

struct FailingResolutionStore {
    error: StorageError,
}

impl ResolutionStore for FailingResolutionStore {
    fn next_id(&self) -> Result<u64, StorageError> {
        Ok(1)
    }
    fn append(&self, _record: &StartupResolutionRecord) -> Result<u64, StorageError> {
        Err(self.error)
    }
    fn read(&self, _id: u64) -> Result<Option<StartupResolutionRecord>, StorageError> {
        Ok(None)
    }
}
