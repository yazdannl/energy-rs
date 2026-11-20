//! Per-pack backoff after a control fault.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// One fault episode: when it ends, and what caused it.
#[derive(Debug, Clone)]
struct Fault {
    until: i64,
    cause: String,
}

/// Remembers which packs must be left alone after a fault, and why.
///
/// A driver/echo fault is followed by exactly one fail-safe hold and then silence: retrying a
/// wedged `acMode` or a restarting device every tick is how the old add-on made it worse. The guard
/// is `Mutex`-backed so the dispatcher can stay `&self` across a tick. A read miss is not a fault
/// and never enters this map; the cause is kept so the journal can say what actually happened.
#[derive(Debug)]
pub struct FaultGuard {
    backoff_s: i64,
    until: Mutex<BTreeMap<String, Fault>>,
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

    /// The cause of this pack's active backoff, if it is still blocking at `now`.
    pub fn blocker(&self, pack_id: &str, now: i64) -> Option<String> {
        self.until.lock().ok().and_then(|until| {
            until
                .get(pack_id)
                .filter(|fault| now < fault.until)
                .map(|fault| fault.cause.clone())
        })
    }

    /// Whether this pack is still backing off at `now`.
    pub fn blocking(&self, pack_id: &str, now: i64) -> bool {
        self.blocker(pack_id, now).is_some()
    }

    /// Start (or extend) the backoff for one pack, remembering the fault that caused it.
    pub fn arm(&self, pack_id: &str, now: i64, cause: &str) {
        if let Ok(mut until) = self.until.lock() {
            until.insert(
                pack_id.to_owned(),
                Fault {
                    until: now.saturating_add(self.backoff_s),
                    cause: cause.to_owned(),
                },
            );
        }
    }

    /// A verified write proves the device is healthy again: stop suppressing this pack.
    pub fn clear(&self, pack_id: &str) {
        if let Ok(mut until) = self.until.lock() {
            until.remove(pack_id);
        }
    }

    /// Forget the packs whose backoff has expired.
    pub fn prune(&self, now: i64) {
        if let Ok(mut until) = self.until.lock() {
            until.retain(|_, fault| now < fault.until);
        }
    }

    /// The packs backing off at `now` (for logs and tests).
    pub fn blocked(&self, now: i64) -> Vec<String> {
        self.until.lock().map_or_else(
            |_| Vec::new(),
            |until| {
                until
                    .iter()
                    .filter(|(_, fault)| now < fault.until)
                    .map(|(pack_id, _)| pack_id.clone())
                    .collect()
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::FaultGuard;

    #[test]
    fn a_fault_blocks_only_the_pack_that_faulted_and_a_verified_write_clears_it() {
        let guard = FaultGuard::new(10);
        guard.arm("A", 100, "echo mismatch");
        assert!(guard.blocking("A", 105));
        assert!(!guard.blocking("B", 105));
        assert_eq!(guard.blocker("A", 105).as_deref(), Some("echo mismatch"));
        guard.clear("A");
        assert!(!guard.blocking("A", 105));
    }
}
