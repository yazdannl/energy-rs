//! The operator's PV array, re-read from the settings on every tick.

use std::sync::Arc;

use energy_core::plan_source::{PvConfig, PvSource};
use energy_ledger::Ledger;

/// The PV configuration, re-read from the settings on every tick.
pub struct SettingsPv {
    ledger: Arc<Ledger>,
}

impl SettingsPv {
    /// A source over `ledger`.
    pub fn new(ledger: Arc<Ledger>) -> Self {
        Self { ledger }
    }

    /// A shared source.
    pub fn shared(ledger: Arc<Ledger>) -> Arc<Self> {
        Arc::new(Self::new(ledger))
    }
}

impl PvSource for SettingsPv {
    fn pv(&self) -> PvConfig {
        crate::bootstrap::config::pv(&self.ledger)
    }
}
