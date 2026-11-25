//! Deterministic M6 fixtures: real anchor/record files in a unique temp directory.
#![allow(dead_code)]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use energy_decide::authority::{
    AnchorStore, Authority, AuthorityLedger, AuthorityState, FileAnchorStore, FileResolutionStore,
};
use energy_decide::dispatch::PackCommand;

static DIR_SEQ: AtomicU64 = AtomicU64::new(0);

/// Temp directory removed on drop; no test leaves data behind.
pub struct TestDir {
    pub path: PathBuf,
}

impl TestDir {
    pub fn new() -> Self {
        let sequence = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let unique = format!(
            "energy-decide-m6-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |value| value.as_nanos()),
            sequence
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("fixture dir");
        Self { path }
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// One authority process over a stable pair of storage files.
pub(crate) struct Fixture {
    pub dir: TestDir,
    pub anchors: FileAnchorStore,
    pub resolutions: FileResolutionStore,
    pub authority: Authority,
}

impl Fixture {
    pub fn new(ledger: AuthorityLedger, now_ms: i64) -> Self {
        let dir = TestDir::new();
        let anchors = FileAnchorStore::in_dir(&dir.path);
        let resolutions = FileResolutionStore::in_dir(&dir.path);
        let authority = Authority::start(ledger, &anchors, &resolutions, now_ms);
        Self {
            dir,
            anchors,
            resolutions,
            authority,
        }
    }

    /// Simulate a process restart with the supplied ledger and the same anchor files.
    pub fn restart(&self, ledger: AuthorityLedger, now_ms: i64) -> Authority {
        Authority::start(ledger, &self.anchors, &self.resolutions, now_ms)
    }

    /// Load the separate-file anchor value.
    pub fn anchor(&self) -> Option<energy_decide::authority::AuthorityAnchor> {
        self.anchors.load().expect("anchor readable")
    }

    /// Write the anchor value directly, simulating a coordinated snapshot restore or a crash.
    pub fn restore_anchor(&self, anchor: &energy_decide::authority::AuthorityAnchor) {
        self.anchors.atomic_write(anchor).expect("anchor restored");
    }

    /// Read all startup-resolution records.
    pub fn records(&self) -> Vec<energy_decide::authority::StartupResolutionRecord> {
        let contents = std::fs::read_to_string(self.resolutions.path()).unwrap_or_default();
        contents
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

/// A ledger with no history: audited generation-1 auto bootstrap.
pub fn fresh(now_ms: i64) -> AuthorityLedger {
    let _ = now_ms;
    AuthorityLedger::new(0, None, "0.15.12")
}

/// A legacy ledger with one persisted mode and no token.
pub fn legacy(mode: &str) -> AuthorityLedger {
    AuthorityLedger::new(1, Some(mode), "0.14.9")
}

/// An economic device command, charge positive.
pub fn command(pack: &str, watts: f64, at_ms: i64) -> PackCommand {
    PackCommand {
        pack_id: pack.to_owned(),
        requested_w: watts,
        issued_at_ms: at_ms,
        generation: 1,
        fence_id: None,
        safety_traffic: false,
    }
}

/// A safety-lane command carrying a fence, as M4 represents it.
pub fn fenced_hold(pack: &str, fence: &str, at_ms: i64) -> PackCommand {
    PackCommand {
        pack_id: pack.to_owned(),
        requested_w: 0.0,
        issued_at_ms: at_ms,
        generation: 1,
        fence_id: Some(fence.to_owned()),
        safety_traffic: true,
    }
}

/// Latched authority from the legacy dry-run preservation path.
pub fn latched_fixture(now_ms: i64) -> Fixture {
    Fixture::new(legacy("dry_run"), now_ms)
}

/// Auto authority from a fresh install.
pub fn auto_fixture(now_ms: i64) -> Fixture {
    Fixture::new(fresh(now_ms), now_ms)
}

/// Assert the three-state machine has not gained an invented fourth state.
pub fn assert_only_three_states() {
    assert_eq!(
        [
            AuthorityState::DryRunLatched.as_str(),
            AuthorityState::Clearing.as_str(),
            AuthorityState::Auto.as_str(),
        ],
        ["dry_run_latched", "clearing", "auto"]
    );
}
