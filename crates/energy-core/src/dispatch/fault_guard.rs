//! Per-pack backoff after a control fault.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Remembers which packs must be left alone after a fault.
///
/// A driver/echo fault is followed by exactly one fail-safe hold and then silence: retrying a
/// wedged `acMode` or a restarting device every tick is how the old add-on made it worse. The guard
/// is `Mutex`-backed so the dispatcher can stay `&self` across a tick.
#[derive(Debug)]
pub struct FaultGuard {
    backoff_s: i64,
    until: Mutex<BTreeMap<String, i64>>,
}

impl FaultGuard {
    /// A guard with a fixed backoff.
    pub fn new(backoff_s: i64) -> Self {
        Self {
            backoff_s: backoff_s.max(0),
            until: Mutex::new(BTreeMap::new()),
        }
    }

    /// The configured backoff, seconds.
    pub fn backoff_s(&self) -> i64 {
        self.backoff_s
    }

    /// Whether this pack is still backing off at `now`.
    pub fn blocking(&self, pack_id: &str, now: i64) -> bool {
        self.until
            .lock()
            .is_ok_and(|until| until.get(pack_id).is_some_and(|at| now < *at))
    }

    /// Start (or extend) the backoff for one pack.
    pub fn arm(&self, pack_id: &str, now: i64) {
        if let Ok(mut until) = self.until.lock() {
            until.insert(pack_id.to_owned(), now.saturating_add(self.backoff_s));
        }
    }

    /// Forget the packs whose backoff has expired.
    pub fn prune(&self, now: i64) {
        if let Ok(mut until) = self.until.lock() {
            until.retain(|_, at| now < *at);
        }
    }

    /// The packs backing off at `now` (for logs and tests).
    pub fn blocked(&self, now: i64) -> Vec<String> {
        self.until.lock().map_or_else(
            |_| Vec::new(),
            |until| {
                until
                    .iter()
                    .filter(|(_, at)| now < **at)
                    .map(|(pack_id, _)| pack_id.clone())
                    .collect()
            },
        )
    }
}
