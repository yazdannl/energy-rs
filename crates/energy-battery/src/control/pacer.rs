//! Per-device write pacing.

use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::control::pace_claim::PaceClaim;

/// Minimum spacing between two writes to the same device: the device drops a command that follows
/// another within ~1.3 s (25 % lost at 1/s, none at 1 per 2 s).
pub const MIN_WRITE_SPACING: Duration = Duration::from_secs(2);

/// The earliest instant the next write may start; `None` before the first write.
#[derive(Debug, Default)]
pub(super) struct PaceState {
    pub(super) not_before: Option<Instant>,
}

/// Serializes writes to one device and keeps them at least [`MIN_WRITE_SPACING`] apart.
///
/// The claim *is* the in-flight slot: a second writer waits on the same mutex, and the next allowed
/// instant is recorded when the claim drops, so the following write is 2 s after the previous one
/// finished rather than 2 s after it started. [`Pacer::defer`] pushes that instant further out when
/// the device needs to be left alone (a restart or a wedged AC path).
#[derive(Debug)]
pub struct Pacer {
    spacing: Duration,
    state: Mutex<PaceState>,
}

impl Pacer {
    pub fn new(spacing: Duration) -> Self {
        Self {
            spacing,
            state: Mutex::new(PaceState::default()),
        }
    }

    pub fn spacing(&self) -> Duration {
        self.spacing
    }

    /// Wait until the next write is legal and claim it.
    pub async fn claim(&self) -> PaceClaim<'_> {
        let state = self.state.lock().await;
        if let Some(not_before) = state.not_before {
            let now = Instant::now();
            if now < not_before {
                tokio::time::sleep(not_before - now).await;
            }
        }
        PaceClaim::new(state, self.spacing)
    }

    /// Hold every later claim back by at least `delay` from now.
    pub async fn defer(&self, delay: Duration) {
        let mut state = self.state.lock().await;
        let candidate = Instant::now() + delay;
        if state.not_before.is_none_or(|current| current < candidate) {
            state.not_before = Some(candidate);
        }
    }
}

impl Default for Pacer {
    fn default() -> Self {
        Self::new(MIN_WRITE_SPACING)
    }
}
