//! The policy in force, shared between the engine and the plan source.

use std::sync::{Arc, RwLock};

use crate::plan::Policy;

/// The live policy: settings plus the pool's basis, refreshed by the engine.
#[derive(Debug, Clone, Default)]
pub struct PolicyHandle(Arc<RwLock<Policy>>);

impl PolicyHandle {
    /// A handle over `policy`.
    pub fn new(policy: Policy) -> Self {
        Self(Arc::new(RwLock::new(policy)))
    }

    /// A shared handle.
    pub fn shared(policy: Policy) -> Arc<Self> {
        Arc::new(Self::new(policy))
    }

    /// The policy in force.
    pub fn get(&self) -> Policy {
        self.0.read().map(|policy| *policy).unwrap_or_default()
    }

    /// Replace it.
    pub fn set(&self, policy: Policy) {
        if let Ok(mut slot) = self.0.write() {
            *slot = policy;
        }
    }
}
