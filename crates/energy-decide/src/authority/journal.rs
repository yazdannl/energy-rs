//! Deterministic integrity chain for authority events and token digests.

use crate::authority::AuthorityEvent;

/// FNV-1a over UTF-8 bytes; deterministic across restarts, no hidden per-process key.
pub(crate) fn hash_bytes(seed: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(seed, |mut hash, byte| {
        hash ^= u64::from(*byte);
        hash.wrapping_mul(0x100_0000_01b3)
    })
}

/// Stable digest for a token seed and generation.
pub(crate) fn hash_text(text: &str) -> String {
    format!(
        "{:016x}",
        hash_bytes(0xcbf2_9ce4_8422_2325, text.as_bytes())
    )
}

/// Hash one event onto the previous chain head.
pub(crate) fn hash_event(prev: u64, event: &AuthorityEvent) -> u64 {
    let payload = format!(
        "{}|{}|{}|{}|{}",
        event.id,
        event.at_ms,
        event.actor,
        event.why,
        event.kind.family()
    );
    hash_bytes(prev, payload.as_bytes())
}

/// Verify every durable event is contiguous and hash-chained to the current head.
pub(crate) fn verify_chain(events: &[AuthorityEvent]) -> bool {
    let mut prev = 0_u64;
    for (index, event) in events.iter().enumerate() {
        let expected_id = u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1);
        if event.id != expected_id {
            return false;
        }
        if hash_event(prev, event) != event.hash || event.prev_hash != prev {
            return false;
        }
        prev = event.hash;
    }
    true
}
