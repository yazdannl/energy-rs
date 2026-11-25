//! Execution-time result of a queued intent; refusals are rows, never silent drops.

use crate::authority::{AuthorityRefusal, EntryPoint};
use crate::dispatch::PackCommand;

/// What happened when a queued intent reached execution.
#[derive(Debug, Clone)]
pub enum IntentOutcome {
    /// The execution-time gate authorized this exact command.
    Authorized { command: PackCommand },
    /// Refused with a typed reason, recorded in the audit log.
    Refused {
        intent_id: u64,
        reason: AuthorityRefusal,
        entry: EntryPoint,
    },
}

impl IntentOutcome {
    /// True only when the execution-time gate authorized the command.
    pub fn authorized(&self) -> bool {
        matches!(self, Self::Authorized { .. })
    }
}
