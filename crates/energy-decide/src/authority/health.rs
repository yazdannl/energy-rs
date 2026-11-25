//! Exact state-to-health mapping, including the loud recovery text.

use crate::authority::{
    ACTIVE_TEXT, AuthorityPhase, AuthorityState, CLEARING_TEXT, CONTINUITY_BANNER, HealthReport,
    LATCHED_TEXT, RECOVERY_TEXT, StartupResolution,
};

/// Build the one health report used by both dashboard and API.
pub(crate) fn report(
    state: AuthorityState,
    phase: AuthorityPhase,
    resolution: StartupResolution,
) -> HealthReport {
    if resolution == StartupResolution::RecoveryRequired || phase == AuthorityPhase::Invalid {
        return HealthReport {
            codes: vec![
                "protection=none",
                "battery_unmanaged=true",
                "warning=latch_token_invalid",
            ],
            text: RECOVERY_TEXT,
            banner: CONTINUITY_BANNER,
            safety_active: false,
            battery_unmanaged: true,
        };
    }
    match state {
        AuthorityState::DryRunLatched => HealthReport {
            codes: vec![
                "protection=none",
                "battery_unmanaged=true",
                "warning=no_protection_writes_forbidden",
            ],
            text: LATCHED_TEXT,
            banner: CONTINUITY_BANNER,
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
            banner: CONTINUITY_BANNER,
            safety_active: false,
            battery_unmanaged: true,
        },
        AuthorityState::Auto => HealthReport {
            codes: vec!["protection=active", "battery_unmanaged=false"],
            text: ACTIVE_TEXT,
            banner: CONTINUITY_BANNER,
            safety_active: true,
            battery_unmanaged: false,
        },
    }
}
