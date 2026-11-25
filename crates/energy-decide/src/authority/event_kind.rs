//! Closed authority event shapes; no variant can carry a device command or setpoint.

use crate::authority::{AuthorityState, BootstrapReason, EntryPoint, StartupResolution};

/// Narrow authority ledger event kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityEventKind {
    /// Sealed initialization/recovery token event.
    LatchTokenInitialized {
        reason: BootstrapReason,
        generation: u64,
        previous_mode: Option<String>,
        source_release: String,
        nonce: u64,
    },
    /// Logged authority state transition.
    AuthorityTransition {
        from: AuthorityState,
        to: AuthorityState,
        reason: String,
        clearance_id: Option<String>,
    },
    /// Operator clearance entry, including the barrier generation.
    ClearanceEntry {
        clearance_id: String,
        generation: u64,
    },
    /// Per-pack barrier result, without any command payload.
    BarrierResult { pack_id: String, generation: u64 },
    /// A path tried to change or preserve the latch and was recorded.
    DryRunLatchPreserved { path: String },
    /// Universal startup-resolution case and record reference.
    StartupResolution {
        case: StartupResolution,
        record_id: u64,
    },
    /// Non-authoritative attempt or operator journal note.
    JournalNote { text: String },
    /// Entry point that requested a write was gated.
    GateRefusal { path: EntryPoint, reason: String },
}

impl AuthorityEventKind {
    /// Event family used by provenance checks; no free-form command field exists.
    pub fn family(&self) -> &'static str {
        match self {
            Self::LatchTokenInitialized { .. } => "latch_token_initialized",
            Self::AuthorityTransition { .. } => "authority_transition",
            Self::ClearanceEntry { .. } => "clearance_entry",
            Self::BarrierResult { .. } => "barrier_result",
            Self::DryRunLatchPreserved { .. } => "dry_run_latch_preserved",
            Self::StartupResolution { .. } => "startup_resolution",
            Self::JournalNote { .. } => "journal_note",
            Self::GateRefusal { .. } => "gate_refusal",
        }
    }
}
