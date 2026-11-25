//! M6 write authority: who may write, how that authority is established and revoked.
//!
//! There is exactly ONE gate (`gate::decide`). Mode endpoints, legacy aliases, generic
//! websocket passthrough, schedules, watchdog, internal recovery, tick dispatch,
//! maintenance/learning persistence, queued intents and manual state edits all call it.
//!
//! LIMITATION (SPEC/LIMITATIONS): the separate-file anchor detects database-only
//! replace/copy/rollback, a missing or mismatched anchor and an incomplete journal. It
//! cannot detect a coherent rollback of BOTH ledger and anchor (a full add-on snapshot
//! restore): no TPM, remote attestation or signing account exists on this host. Such a
//! restore lands in `reconciled` with the universal banner and a journalled resolution,
//! never a false claim of proven continuity.

pub mod anchor;
pub mod anchor_store;
pub mod atomic_file;
pub mod audit_event;
pub mod audit_log;
pub mod authority_event;
pub mod authority_ops;
pub mod bootstrap_authority_tx;
pub mod bootstrap_capability;
pub mod bootstrap_reason;
pub mod bootstrap_tx_error;
pub mod caller;
pub mod clearance_command;
pub mod clearance_permit;
pub mod committed_bootstrap;
pub mod constants;
pub mod decision;
pub mod entry_point;
pub mod event_kind;
pub mod file_anchor_store;
pub mod file_resolution_store;
pub mod gate;
pub mod health;
pub mod health_report;
pub mod initialization_event;
pub mod intent_outcome;
pub mod journal;
pub mod latch;
pub mod ledger;
pub mod owner;
pub mod provenance;
pub mod queued_intent;
pub mod reconcile_permit;
pub mod refusal;
pub mod resolution;
pub mod resolution_store;
pub mod startup;
pub mod startup_input;
pub mod startup_outcome;
pub mod startup_resolution_record;
pub mod startup_steps;
pub mod state;
pub mod storage_error;
pub mod token;
pub mod write_class;
pub mod write_request;

pub use anchor::AuthorityAnchor;
pub use anchor_store::AnchorStore;
pub use audit_event::AuditEvent;
pub use audit_log::AuditLog;
pub use authority_event::AuthorityEvent;
pub use bootstrap_authority_tx::BootstrapAuthorityTx;
pub use bootstrap_capability::BootstrapCapability;
pub use bootstrap_reason::BootstrapReason;
pub use bootstrap_tx_error::BootstrapTxError;
pub use caller::Caller;
pub use clearance_command::ClearanceCommand;
pub use clearance_permit::ClearancePermit;
pub use committed_bootstrap::CommittedBootstrap;
pub use decision::GateOutcome;
pub use entry_point::EntryPoint;
pub use event_kind::AuthorityEventKind;
pub use file_anchor_store::FileAnchorStore;
pub use file_resolution_store::FileResolutionStore;
pub use health_report::HealthReport;
pub use initialization_event::InitializationOrRecoveryEvent;
pub use intent_outcome::IntentOutcome;
pub use latch::Latch;
pub use ledger::AuthorityLedger;
pub use owner::Authority;
pub use queued_intent::QueuedIntent;
pub use reconcile_permit::ReconcilePermit;
pub use refusal::AuthorityRefusal;
pub use resolution::StartupResolution;
pub use resolution_store::ResolutionStore;
pub use startup_outcome::StartupOutcome;
pub use startup_resolution_record::StartupResolutionRecord;
pub use state::AuthorityState;
pub use storage_error::StorageError;
pub use token::LatchToken;
pub use write_class::WriteClass;
pub use write_request::WriteRequest;

pub use authority_ops::command_class;
pub(crate) use constants::WRITE_AUTHORITY_SCHEMA;
pub use constants::{
    ACTIVE_TEXT, CLEARANCE_WINDOW_MS, CLEARING_TEXT, CONTINUITY_BANNER, LATCHED_TEXT,
    PROTECTION_ACTIVE_SESSION_MS, RECOVERY_TEXT,
};
pub use phase::AuthorityPhase;

pub mod phase;
