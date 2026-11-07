//! Shared fixtures: a throwaway database directory and small pack/sample helpers.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use energy_ledger::{Ledger, LedgerConfig, Pack, RetentionPolicy, Sample};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A temporary directory holding one SQLite file, removed on drop.
pub struct TempDatabase {
    dir: PathBuf,
    path: PathBuf,
}

impl TempDatabase {
    pub fn new() -> Self {
        let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "energy-ledger-{}-{stamp}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("the temp directory is writable");
        let path = dir.join("ledger.db");
        Self { dir, path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn ledger(&self) -> Ledger {
        Ledger::open(&LedgerConfig::at(&self.path)).expect("the ledger opens")
    }

    pub fn ledger_with(&self, retention: RetentionPolicy) -> Ledger {
        let config = LedgerConfig::at(&self.path).with_retention(retention);
        Ledger::open(&config).expect("the ledger opens")
    }
}

impl Default for TempDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Register a pack and return its id.
pub fn register_pack(ledger: &Ledger, sn: &str) -> i64 {
    ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new(sn, 1.92, 0))
        .expect("the pack is stored")
        .id
}

/// A 1-minute sample.
pub fn sample(pack_id: i64, ts: i64, power_w: f64, soc_pct: Option<f64>) -> Sample {
    Sample::new(pack_id, ts, power_w, soc_pct)
}

/// Write samples in one batch.
pub fn write(ledger: &Ledger, samples: &[Sample]) {
    ledger
        .measurements()
        .expect("measurements")
        .write_many(samples)
        .expect("the samples are stored");
}
