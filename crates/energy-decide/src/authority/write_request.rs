//! One request presented to the single authority gate.

use crate::authority::{Caller, WriteClass};

/// Caller plus requested write class; carries no device command in control-plane classes.
#[derive(Debug, Clone)]
pub struct WriteRequest {
    /// Who asks, through which entry point, why and when.
    pub caller: Caller,
    /// What class of write is requested.
    pub class: WriteClass,
}

impl WriteRequest {
    /// Build a request.
    pub fn new(caller: Caller, class: WriteClass) -> Self {
        Self { caller, class }
    }
}
