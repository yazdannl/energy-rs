//! Remaining budget arithmetic for one end-to-end per-pack attempt.

use std::time::Instant;

/// Milliseconds of the single budget still available.
pub fn remaining_ms(started: Instant, budget_ms: u64) -> u64 {
    let used = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    budget_ms.saturating_sub(used)
}

/// Elapsed milliseconds since an instant.
pub fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
