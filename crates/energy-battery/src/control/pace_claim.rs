//! The single in-flight write slot.

use std::time::Duration;

use tokio::sync::MutexGuard;
use tokio::time::Instant;

use crate::control::pacer::PaceState;

/// Proof that the caller may write now and holds the single in-flight slot.
///
/// Dropping it records the completion instant for the next claimant.
#[derive(Debug)]
pub struct PaceClaim<'a> {
    state: MutexGuard<'a, PaceState>,
    spacing: Duration,
}

impl<'a> PaceClaim<'a> {
    pub(super) fn new(state: MutexGuard<'a, PaceState>, spacing: Duration) -> Self {
        Self { state, spacing }
    }
}

impl Drop for PaceClaim<'_> {
    fn drop(&mut self) {
        self.state.not_before = Some(Instant::now() + self.spacing);
    }
}
