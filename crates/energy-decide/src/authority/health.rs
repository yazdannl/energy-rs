//! Exact state-to-health mapping, including the loud, distinct recovery verdict.

use crate::authority::{
    ACTIVE_TEXT, AuthorityPhase, AuthorityState, CLEARING_TEXT, CONTINUITY_BANNER, HealthReport,
    LATCHED_TEXT, RECOVERY_TEXT, RecoveryEvidence, StartupResolution,
};

/// Build the one health report used by both dashboard and API.
pub(crate) fn report(
    state: AuthorityState,
    phase: AuthorityPhase,
    resolution: StartupResolution,
    evidence: Option<&RecoveryEvidence>,
) -> HealthReport {
    if resolution == StartupResolution::RecoveryRequired || phase == AuthorityPhase::Invalid {
        return recovery_report(evidence);
    }
    match state {
        AuthorityState::DryRunLatched => HealthReport {
            codes: vec![
                "protection=none",
                "battery_unmanaged=true",
                "warning=no_protection_writes_forbidden",
            ],
            text: LATCHED_TEXT,
            banner: CONTINUITY_BANNER.to_owned(),
            condition: "dry_run_latched",
            recovery: None,
            safety_active: false,
            battery_unmanaged: true,
        },
        AuthorityState::Clearing => HealthReport {
            codes: vec![
                "protection=neutralisation_only",
                "battery_unmanaged=true",
                "warning=clearance_in_progress",
            ],
            text: CLEARING_TEXT,
            banner: CONTINUITY_BANNER.to_owned(),
            condition: "clearing",
            recovery: None,
            safety_active: false,
            battery_unmanaged: true,
        },
        AuthorityState::Auto => HealthReport {
            codes: vec!["protection=active", "battery_unmanaged=false"],
            text: ACTIVE_TEXT,
            banner: CONTINUITY_BANNER.to_owned(),
            condition: "auto",
            recovery: None,
            safety_active: true,
            battery_unmanaged: false,
        },
    }
}

fn recovery_report(evidence: Option<&RecoveryEvidence>) -> HealthReport {
    let detail = evidence.map_or_else(
        || "reason=latch_token_invalid".to_owned(),
        RecoveryEvidence::detail,
    );
    HealthReport {
        codes: vec![
            "protection=none",
            "battery_unmanaged=true",
            "warning=authority_recovery_required",
        ],
        text: RECOVERY_TEXT,
        banner: format!("{CONTINUITY_BANNER} Reconciliation failed: {detail}"),
        condition: "recovery_required",
        recovery: evidence.cloned(),
        safety_active: false,
        battery_unmanaged: true,
    }
}
