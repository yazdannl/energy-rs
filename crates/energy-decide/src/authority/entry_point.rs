//! Every path that can ask for a write, named so each one can be proven gated.

/// All entry points that can request an authority decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryPoint {
    Startup,
    BootstrapTransaction,
    ModeEndpoint,
    LegacyAlias,
    AuditExpiry,
    ConfigReload,
    ApiRequest,
    GenericWsPassthrough,
    Schedule,
    WatchdogRestart,
    InternalRecovery,
    TickDispatch,
    MaintenancePersistence,
    LearningPersistence,
    ClearanceCommand,
    QueuedIntentExecution,
    ManualStateEdit,
    DashboardMutation,
    LedgerWrite,
    AnchorUpdate,
    StartupResolutionWrite,
}

impl EntryPoint {
    /// Stable path name used by `dry_run_latch_preserved{path}` and the audit log.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::BootstrapTransaction => "bootstrap",
            Self::ModeEndpoint => "mode_migration",
            Self::LegacyAlias => "legacy_alias",
            Self::AuditExpiry => "audit_expiry",
            Self::ConfigReload => "config_reload",
            Self::ApiRequest => "api_request",
            Self::GenericWsPassthrough => "generic_ws_passthrough",
            Self::Schedule => "schedule",
            Self::WatchdogRestart => "watchdog_restart",
            Self::InternalRecovery => "internal_recovery",
            Self::TickDispatch => "tick_dispatch",
            Self::MaintenancePersistence => "maintenance_persistence",
            Self::LearningPersistence => "learning_persistence",
            Self::ClearanceCommand => "clearance_command",
            Self::QueuedIntentExecution => "queued_intent_execution",
            Self::ManualStateEdit => "manual_state_edit",
            Self::DashboardMutation => "dashboard_mutation",
            Self::LedgerWrite => "ledger_write",
            Self::AnchorUpdate => "anchor_update",
            Self::StartupResolutionWrite => "startup_resolution_write",
        }
    }
}
