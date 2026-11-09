//! The operator policy, re-read from the settings on every tick.

use std::sync::Arc;

use energy_core::plan::Policy;
use energy_core::plan_source::PolicySource;
use energy_ledger::Ledger;

/// The operator policy, re-read from the settings on every tick.
pub struct SettingsPolicy {
    ledger: Arc<Ledger>,
}

impl SettingsPolicy {
    /// A source over `ledger`.
    pub fn new(ledger: Arc<Ledger>) -> Self {
        Self { ledger }
    }

    /// A shared handle.
    pub fn shared(ledger: Arc<Ledger>) -> Arc<Self> {
        Arc::new(Self::new(ledger))
    }
}

impl PolicySource for SettingsPolicy {
    fn policy(&self) -> Policy {
        crate::bootstrap::config::policy(&self.ledger)
    }
}
