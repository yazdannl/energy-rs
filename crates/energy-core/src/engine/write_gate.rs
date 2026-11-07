//! The one authority gate every hardware command passes through.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::safety::Authority;

/// Mode bits in the packed gate state; every generation step is one `MODE_BITS + 1`.
const MODE_BITS: u64 = 0b11;
/// One generation step: a token from an older generation never matches again.
const STEP: u64 = 0b100;

/// The write authority plus a control generation, in one atomic word.
///
/// Two independent questions are answered here and nowhere else: *may anything reach a device right
/// now* ([`Authority::may_write`]) and *is this command still from the current control generation*.
/// A tick captures [`token`](Self::token) before it reads anything and hands it to the
/// [`GuardedSink`](crate::engine::GuardedSink); every transport write compares it against the live
/// word. An emergency hold or a mode change bumps the generation first, so a command a resumed tick
/// queued before the bump can never be written afterwards, and a `dry_run`/`advisory` engine can
/// never write at all, not even a hold.
///
/// The two live in one word on purpose: a mode change and a fence cannot be observed separately, so
/// there is no window in which the mode is already `dry_run` but a stale token still admits a write.
#[derive(Debug)]
pub struct WriteGate {
    state: AtomicU64,
}

impl WriteGate {
    /// A gate in `mode`, generation 1.
    pub fn new(mode: Authority) -> Self {
        Self {
            state: AtomicU64::new(bits(mode) | STEP),
        }
    }

    /// The current word: mode bits plus generation. Captured at the start of a control action.
    pub fn token(&self) -> u64 {
        self.state.load(Ordering::SeqCst)
    }

    /// The authority in force.
    pub fn mode(&self) -> Authority {
        from_bits(self.token() & MODE_BITS)
    }

    /// Bump the generation, keeping the mode; the returned token is the new one.
    ///
    /// Called before anything that supersedes queued work: an emergency hold, a shutdown hold, a
    /// boot recovery pass.
    pub fn fence(&self) -> u64 {
        self.state.fetch_add(STEP, Ordering::SeqCst) + STEP
    }

    /// Bump the generation and change the mode in one atomic step.
    pub fn transition(&self, mode: Authority) -> u64 {
        let mut current = self.state.load(Ordering::SeqCst);
        loop {
            let next = ((current & !MODE_BITS) + STEP) | bits(mode);
            match self
                .state
                .compare_exchange(current, next, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return next,
                Err(actual) => current = actual,
            }
        }
    }

    /// Whether a command carrying `token` may reach a device now.
    ///
    /// False when anything moved since the token was captured, and false outside `auto`: both
    /// answers come from the same word.
    pub fn admit(&self, token: u64) -> bool {
        let current = self.token();
        current == token && from_bits(current & MODE_BITS).may_write()
    }
}

const fn bits(mode: Authority) -> u64 {
    match mode {
        Authority::Advisory => 0,
        Authority::DryRun => 1,
        Authority::Auto => 2,
    }
}

/// An unrecognised mode word is `advisory`: fail closed, never guess `auto`.
const fn from_bits(bits: u64) -> Authority {
    match bits {
        2 => Authority::Auto,
        1 => Authority::DryRun,
        _ => Authority::Advisory,
    }
}
