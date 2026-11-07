//! The parts of a snapshot the engine does not discover itself.
//!
//! The engine owns control (mode, plan, packs, meter, last tick); Home Assistant discovery and the
//! price store are the server's, so the server publishes them through this one read-only seam
//! instead of the engine re-reading `energy/get_prefs` on every tick. Every method has a safe
//! default, so a test or a server without discovery only implements what it actually has.

use crate::stream::{FaultSnapshot, SiteSnapshot};

/// Site discovery and the faults only the server can see (HA link, stale prices).
pub trait RuntimeView: Send + Sync {
    /// The discovered energy dashboard, as last resolved.
    fn site(&self) -> SiteSnapshot {
        SiteSnapshot::default()
    }

    /// Faults the server opened (for example `ha_disconnected` or `prices_stale`).
    fn faults(&self) -> Vec<FaultSnapshot> {
        Vec::new()
    }
}
