//! Deterministic fakes for M4 dispatch tests; no live I/O.
#![allow(dead_code)] // Shared across several integration-test binaries.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod behavior;
mod clock;
mod fake_io;

pub use behavior::Behavior;
pub use clock::ImmediateClock;
pub use fake_io::FakeIo;

use std::sync::Mutex;

use energy_decide::authority::{
    AnchorStore, Authority, AuthorityAnchor, AuthorityLedger, ReconcilePermit, ResolutionStore,
    StartupResolutionRecord, StorageError,
};
use energy_decide::dispatch::FlowReport;

struct MemoryAnchor {
    value: Mutex<Option<AuthorityAnchor>>,
}

impl AnchorStore for MemoryAnchor {
    fn load(&self) -> Result<Option<AuthorityAnchor>, StorageError> {
        Ok(self.value.lock().expect("anchor lock").clone())
    }

    fn atomic_write(&self, anchor: &AuthorityAnchor) -> Result<(), StorageError> {
        *self.value.lock().expect("anchor lock") = Some(anchor.clone());
        Ok(())
    }
}

struct MemoryResolutions {
    next: Mutex<u64>,
}

impl ResolutionStore for MemoryResolutions {
    fn next_id(&self) -> Result<u64, StorageError> {
        let mut next = self.next.lock().expect("resolution lock");
        let id = *next;
        *next = next.saturating_add(1);
        Ok(id)
    }

    fn append(&self, _record: &StartupResolutionRecord) -> Result<u64, StorageError> {
        self.next_id()
    }

    fn read(&self, _id: u64) -> Result<Option<StartupResolutionRecord>, StorageError> {
        Ok(None)
    }
}

/// Unforgeable reconcile capability obtained from the authority owner, never from an API caller.
pub fn reconcile_permit() -> ReconcilePermit {
    let anchors = MemoryAnchor {
        value: Mutex::new(None),
    };
    let resolutions = MemoryResolutions {
        next: Mutex::new(1),
    };
    let authority = Authority::start(
        AuthorityLedger::new(0, None, "0.15.12"),
        &anchors,
        &resolutions,
        1_000,
    );
    authority
        .issue_reconcile_permit("authority", 1_000, "m4 reconcile")
        .expect("reconcile permit")
}

/// Build a fresh flow report with explicit status and fault context.
pub fn report(
    pack: &str,
    at_ms: i64,
    measured_w: f64,
    ac: i32,
    dc: i32,
    message_id: u32,
) -> FlowReport {
    FlowReport {
        pack_id: pack.to_owned(),
        at_ms,
        measured_w,
        ac_status: ac,
        dc_status: dc,
        fault_level: 0,
        soc_pct: 60.0,
        device_min_soc_pct: 10.0,
        echo: None,
        message_id,
        device_clock_ok: true,
        device_clock_coherent: true,
    }
}
