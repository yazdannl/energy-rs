#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Unit-level proof of the real bootstrap issuance boundary (S329-S331).
//!
//! These tests are inside the crate because `begin` and the capability constructor are
//! deliberately crate-private: durable state alone must not be enough to mint a capability.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::authority::{
    AnchorStore, AuthorityAnchor, AuthorityLedger, BootstrapAuthorityTx, BootstrapIssue,
    BootstrapTxError, FileAnchorStore, IssuedBootstrapCapability,
};

static DIR_SEQ: AtomicU64 = AtomicU64::new(1);

struct SealDir(PathBuf);

impl SealDir {
    fn new(tag: &str) -> Self {
        let unique = format!(
            "energy-seal-{tag}-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |value| value.as_nanos()),
            DIR_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("seal test dir");
        Self(path)
    }
}

impl Drop for SealDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn issued(
    anchors: &FileAnchorStore,
    run_id: u64,
    tx_id: u64,
    nonce: u64,
) -> (AuthorityLedger, IssuedBootstrapCapability) {
    let ledger = AuthorityLedger::new(0, None, "0.15.12");
    let mut anchor = AuthorityAnchor::new(0, "", 0, 0, 0, 0);
    anchor.issue_bootstrap(BootstrapIssue::new(tx_id, run_id, nonce, 1_000));
    anchors.atomic_write(&anchor).expect("issue marker");
    (ledger, IssuedBootstrapCapability::new(run_id, tx_id, nonce))
}

fn ledger_shape(ledger: &AuthorityLedger) -> (u64, usize, u64, Option<u64>) {
    (
        ledger.head_event_id(),
        ledger.events().len(),
        ledger.checkpoint_event_id(),
        ledger.latch().and_then(|latch| latch.token().generation()),
    )
}

fn assert_unchanged(
    anchors: &FileAnchorStore,
    ledger: &AuthorityLedger,
    before_anchor: &AuthorityAnchor,
    before_ledger: (u64, usize, u64, Option<u64>),
) {
    assert_eq!(
        anchors.load().expect("anchor load"),
        Some(before_anchor.clone())
    );
    assert_eq!(ledger_shape(ledger), before_ledger);
}

#[test]
fn s329_second_issuance_from_identical_pre_bootstrap_snapshot_refused() {
    let dir = SealDir::new("second");
    let anchors = FileAnchorStore::in_dir(&dir.0);
    let (ledger, capability) = issued(&anchors, 1, 2, 7);
    let before_anchor = anchors.load().expect("anchor load").expect("anchor");
    assert!(
        !before_anchor
            .bootstrap_issue
            .as_ref()
            .expect("issue")
            .consumed
    );
    let before_ledger = ledger_shape(&ledger);

    let first =
        BootstrapAuthorityTx::begin(ledger.clone(), 1_000, "authority", &anchors, &capability);
    assert!(first.is_ok(), "first issuance must be legitimate");
    let consumed = anchors.load().expect("anchor load").expect("anchor");
    assert!(
        consumed.bootstrap_issue.as_ref().expect("issue").consumed,
        "begin must durably consume the marker before returning"
    );

    // Same snapshot and capability: the second real begin must now be refused.
    let replay =
        BootstrapAuthorityTx::begin(ledger.clone(), 1_000, "authority", &anchors, &capability);
    assert_eq!(
        replay.expect_err("second issuance"),
        BootstrapTxError::ReplayDenied
    );
    assert_eq!(anchors.load().expect("anchor load"), Some(consumed));
    assert_eq!(ledger_shape(&ledger), before_ledger);
}

#[test]
fn s330_foreign_run_capability_refused() {
    let dir = SealDir::new("run");
    let anchors = FileAnchorStore::in_dir(&dir.0);
    let (ledger, _) = issued(&anchors, 1, 2, 7);
    let before_anchor = anchors.load().expect("anchor load").expect("anchor");
    let before_ledger = ledger_shape(&ledger);
    let foreign = IssuedBootstrapCapability::new(99, 2, 7);

    let attempt =
        BootstrapAuthorityTx::begin(ledger.clone(), 1_000, "authority", &anchors, &foreign);
    assert_eq!(
        attempt.expect_err("foreign run"),
        BootstrapTxError::CapabilityDenied
    );
    assert_unchanged(&anchors, &ledger, &before_anchor, before_ledger);
}

#[test]
fn s331_foreign_transaction_capability_refused() {
    let dir = SealDir::new("tx");
    let anchors = FileAnchorStore::in_dir(&dir.0);
    let (ledger, _) = issued(&anchors, 1, 2, 7);
    let before_anchor = anchors.load().expect("anchor load").expect("anchor");
    let before_ledger = ledger_shape(&ledger);
    let substituted = IssuedBootstrapCapability::new(1, 900, 7);

    let attempt =
        BootstrapAuthorityTx::begin(ledger.clone(), 1_000, "authority", &anchors, &substituted);
    assert_eq!(
        attempt.expect_err("foreign transaction"),
        BootstrapTxError::CapabilityDenied
    );
    assert_unchanged(&anchors, &ledger, &before_anchor, before_ledger);
}
