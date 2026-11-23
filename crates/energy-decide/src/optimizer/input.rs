//! Everything M2 needs to optimize; no I/O, no clock, no hardware.

use crate::optimizer::{OptimizerConfig, Slot};

/// A bounded horizon plus the physics that constrain every path through it.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizerInput {
    /// Priced slots in chronological order; `N = slots.len()` is the terminal index.
    pub slots: Vec<Slot>,
    /// Learned-basis physics, site ceiling and solver settings.
    pub config: OptimizerConfig,
}
