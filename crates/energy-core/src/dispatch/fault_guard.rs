//! Per-pack backoff after a control fault, and separate non-fault curtailment pauses.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// A pack that echoed a command but drew no power, or restarted mid-stream, is left alone for
/// minutes. A 10 s breather produced the 20 s re-command loop seen on 2026-11-21.
pub const LONG_FAULT_BACKOFF_S: i64 = 300;

/// A pack that is really flowing but below its command is left alone briefly: this is a confirmed
/// limitation, not a fault, and re-writing it every tick would be the old command storm again.
pub const CURTAILMENT_PAUSE_S: i64 = 120;

/// One suppression episode: when it ends, and why.
#[derive(Debug, Clone)]
struct Episode {
    until: i64,
    cause: String,
}

/// Remembers which packs must be left alone, and why.
///
/// Faults and curtailment pauses are separate maps: a partial safety write during a real fault
/// must never relabel or shorten that fault, even when the pause runs later. A fault cause wins
/// while it is active; the pause cause is visible only after the fault has expired. Both are
/// `Mutex`-backed so the dispatcher can stay `&self` across a tick.
#[derive(Debug)]
pub struct FaultGuard {
    backoff_s: i64,
    faults: Mutex<BTreeMap<String, Episode>>,
    pauses: Mutex<BTreeMap<String, Episode>>,
    /// When this pack's last fault started; retained after expiry so an old streak cannot re-arm it.
    episodes: Mutex<BTreeMap<String, i64>>,
}

impl FaultGuard {
    /// A guard with a fixed backoff.
    pub fn new(backoff_s: i64) -> Self {
        Self {
            backoff_s: backoff_s.max(0),
            faults: Mutex::new(BTreeMap::new()),
            pauses: Mutex::new(BTreeMap::new()),
            episodes: Mutex::new(BTreeMap::new()),
        }
    }

    /// The configured backoff, seconds.
    pub fn backoff_s(&self) -> i64 {
        self.backoff_s
    }

    /// The active suppression cause: a fault wins over a curtailment pause.
    pub fn blocker(&self, pack_id: &str, now: i64) -> Option<String> {
        if let Ok(faults) = self.faults.lock()
            && let Some(fault) = faults.get(pack_id).filter(|fault| now < fault.until)
        {
            return Some(fault.cause.clone());
        }
        self.pauses.lock().ok().and_then(|pauses| {
            pauses
                .get(pack_id)
                .filter(|pause| now < pause.until)
                .map(|pause| pause.cause.clone())
        })
    }

    /// When this pack's last fault episode started, even if the backoff has since expired.
    pub fn episode_start(&self, pack_id: &str) -> Option<i64> {
        self.episodes
            .lock()
            .ok()
            .and_then(|episodes| episodes.get(pack_id).copied())
    }

    /// Whether this pack is still being suppressed at `now`.
    pub fn blocking(&self, pack_id: &str, now: i64) -> bool {
        self.blocker(pack_id, now).is_some()
    }

    /// Start (or extend) the backoff for one pack, remembering the fault that caused it.
    pub fn arm(&self, pack_id: &str, now: i64, cause: &str) {
        self.arm_for(pack_id, now, cause, self.backoff_s);
    }

    /// Back off for an explicit duration, for faults that need minutes rather than one tick.
    pub fn arm_for(&self, pack_id: &str, now: i64, cause: &str, seconds: i64) {
        if let Ok(mut episodes) = self.episodes.lock() {
            episodes.insert(pack_id.to_owned(), now);
        }
        if let Ok(mut faults) = self.faults.lock() {
            faults.insert(
                pack_id.to_owned(),
                Episode {
                    until: now.saturating_add(seconds.max(0)),
                    cause: cause.to_owned(),
                },
            );
        }
    }

    /// A non-fault pause never touches fault state; a later or equal pause just extends its own map.
    pub fn arm_pause(&self, pack_id: &str, now: i64, cause: &str, seconds: i64) {
        let candidate = now.saturating_add(seconds.max(0));
        if let Ok(mut pauses) = self.pauses.lock() {
            let entry = pauses.entry(pack_id.to_owned()).or_insert(Episode {
                until: candidate,
                cause: String::new(),
            });
            if candidate >= entry.until {
                entry.until = candidate;
                entry.cause.clear();
                entry.cause.push_str(cause);
            }
        }
    }

    /// The same long backoff the controller uses for a restarted pack.
    pub fn arm_long(&self, pack_id: &str, now: i64, cause: &str) {
        self.arm_for(pack_id, now, cause, LONG_FAULT_BACKOFF_S);
    }

    /// A verified write proves the device is healthy again: stop suppressing this pack.
    pub fn clear(&self, pack_id: &str) {
        if let Ok(mut faults) = self.faults.lock() {
            faults.remove(pack_id);
        }
        if let Ok(mut pauses) = self.pauses.lock() {
            pauses.remove(pack_id);
        }
    }

    /// Forget the packs whose suppression has expired.
    pub fn prune(&self, now: i64) {
        if let Ok(mut faults) = self.faults.lock() {
            faults.retain(|_, fault| now < fault.until);
        }
        if let Ok(mut pauses) = self.pauses.lock() {
            pauses.retain(|_, pause| now < pause.until);
        }
    }

    /// The packs being suppressed at `now` (for logs and tests).
    pub fn blocked(&self, now: i64) -> Vec<String> {
        let mut ids = Vec::new();
        if let Ok(faults) = self.faults.lock() {
            ids.extend(
                faults
                    .iter()
                    .filter(|(_, fault)| now < fault.until)
                    .map(|(pack_id, _)| pack_id.clone()),
            );
        }
        if let Ok(pauses) = self.pauses.lock() {
            for (pack_id, _) in pauses.iter().filter(|(_, pause)| now < pause.until) {
                if !ids.contains(pack_id) {
                    ids.push(pack_id.clone());
                }
            }
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::{FaultGuard, LONG_FAULT_BACKOFF_S};

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

    #[test]
    fn a_pause_never_relabels_or_shortens_a_late_fault() {
        let guard = FaultGuard::new(10);
        guard.arm_for("A", 0, "scripted no-flow", LONG_FAULT_BACKOFF_S);
        guard.arm_pause("A", 250, "current-limited", 120);
        assert_eq!(guard.episode_start("A"), Some(0));
        assert_eq!(guard.blocker("A", 260).as_deref(), Some("scripted no-flow"));
        assert_eq!(
            guard.blocker("A", LONG_FAULT_BACKOFF_S + 1).as_deref(),
            Some("current-limited")
        );
        guard.arm_pause("A", 300, "current-limited", 10);
        assert_eq!(
            guard.blocker("A", LONG_FAULT_BACKOFF_S + 1).as_deref(),
            Some("current-limited")
        );
    }
}
