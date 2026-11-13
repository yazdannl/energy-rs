//! Where the operator's PV configuration comes from.

use super::PvConfig;

/// Where the PV configuration comes from (the settings, re-read every tick).
pub trait PvSource: Send + Sync {
    /// The configured array and mode.
    fn pv(&self) -> PvConfig;
}
