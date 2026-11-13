//! The PV configuration in force, shared between the engine and the plan source.

use std::sync::{Arc, RwLock};

use super::PvConfig;

/// The live PV configuration: settings, re-read every tick by the engine.
#[derive(Debug, Clone, Default)]
pub struct PvHandle(Arc<RwLock<PvConfig>>);

impl PvHandle {
    /// A handle over `config`.
    pub fn new(config: PvConfig) -> Self {
        Self(Arc::new(RwLock::new(config)))
    }

    /// A shared handle.
    pub fn shared(config: PvConfig) -> Arc<Self> {
        Arc::new(Self::new(config))
    }

    /// The configuration in force.
    pub fn get(&self) -> PvConfig {
        self.0
            .read()
            .map(|config| config.clone())
            .unwrap_or_default()
    }

    /// Replace it.
    pub fn set(&self, config: PvConfig) {
        if let Ok(mut slot) = self.0.write() {
            *slot = config;
        }
    }
}
