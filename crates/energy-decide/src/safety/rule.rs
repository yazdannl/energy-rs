//! The one ordered override list. Solar headroom is deliberately not in it.

/// Ordered safety rules; the first firing command or deferral wins, every firing reason is reported.
///
/// Positions 1-4 follow SPEC M5. Position 3 (SoC floor) and position 5 (ceiling) are
/// universal physical limits promoted into the visible order because SCENARIOS S37/S150
/// require their reasons in precedence order. Positions 8-11 are first-class transitions,
/// 12-14 are outstanding-hazard/ordinary-tier blockers. `Ordinary` is the explicit
/// no-action outcome, never an empty tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    /// Latched cell discharge: verified 0 W.
    CellLatch,
    /// Latched low cell: the minimum bounded charge, or a 0 W hold fallback.
    CellEmergency,
    /// Hard SoC/minSoc floor: no discharge.
    SocFloor,
    /// Projected import breach: protective discharge or charge reduction.
    ImportCeiling,
    /// SoC ceiling / full-charge protection: no charge, no fault escalation.
    FullCharge,
    /// Export/self-consumption hard guard.
    ExportGuard,
    /// Samples older than the 300 s reachability TTL: unreachable until neutral.
    StaleSample,
    /// Samples older than two tick periods: no nonzero command is granted.
    StaleCommand,
    /// Ownership lost: orphan hold.
    OrphanHold,
    /// Standing nonzero command with a changed target.
    CancelRamp,
    /// 120 s post-restart grace.
    RestartGrace,
    /// No verified neutrality after a hold: keep blocking ordinary traffic.
    UnverifiedHold,
    /// Unproven write: ordinary retries blocked, safety lane still active.
    UncertainWrite,
    /// Ordinary fault backoff.
    FaultBackoff,
    /// Two neutral reports show no command is standing.
    NoCommand,
    /// Explicit no-action: the economic request proceeds.
    Ordinary,
}

/// Every rule in execution order; evaluation and reporting both iterate this array.
pub const RULE_ORDER: [Rule; 16] = [
    Rule::CellLatch,
    Rule::CellEmergency,
    Rule::SocFloor,
    Rule::ImportCeiling,
    Rule::FullCharge,
    Rule::ExportGuard,
    Rule::StaleSample,
    Rule::StaleCommand,
    Rule::OrphanHold,
    Rule::CancelRamp,
    Rule::RestartGrace,
    Rule::UnverifiedHold,
    Rule::UncertainWrite,
    Rule::FaultBackoff,
    Rule::NoCommand,
    Rule::Ordinary,
];

impl Rule {
    /// One-based precedence position.
    pub fn position(self) -> usize {
        RULE_ORDER
            .iter()
            .position(|rule| *rule == self)
            .map_or(0, |index| index + 1)
    }

    /// Reason code carried to M4 and the decision record.
    pub fn reason(self) -> &'static str {
        match self {
            Self::CellLatch => "cell_voltage_guard",
            Self::CellEmergency => "cell_voltage_emergency_charge",
            Self::SocFloor => "soc_floor",
            Self::ImportCeiling => "grid_import_ceiling",
            Self::FullCharge => "full_charge_protection",
            Self::ExportGuard => "self_consumption_guard",
            Self::StaleSample => "stale_sample_unreachable",
            Self::StaleCommand => "stale_command_sample",
            Self::OrphanHold => "orphan_hold",
            Self::CancelRamp => "cancel_ramp",
            Self::RestartGrace => "restart_grace",
            Self::UnverifiedHold => "hold_unverified",
            Self::UncertainWrite => "controller_uncertain",
            Self::FaultBackoff => "ordinary_backoff",
            Self::NoCommand => "no_command_needed",
            Self::Ordinary => "no_safety_hazard",
        }
    }

    /// What the rule commands, for the documented precedence table.
    pub fn command(self) -> &'static str {
        match self {
            Self::CellLatch
            | Self::SocFloor
            | Self::FullCharge
            | Self::ExportGuard
            | Self::StaleSample
            | Self::StaleCommand
            | Self::OrphanHold
            | Self::CancelRamp
            | Self::RestartGrace
            | Self::UnverifiedHold => "0 W verified neutral hold",
            Self::CellEmergency => "bounded minimum charge, else 0 W verified hold",
            Self::ImportCeiling => "protective discharge/charge reduction, else 0 W hold",
            Self::UncertainWrite => "0 W hold until neutrality is proven",
            Self::FaultBackoff => "no command; defer ordinary write to retry_at",
            Self::NoCommand | Self::Ordinary => "no safety command",
        }
    }

    /// Verification requirement for the rule's command.
    pub fn verification(self) -> &'static str {
        match self {
            Self::CellEmergency => {
                "echo plus cell voltage at resume threshold within 180 s; else verified hold"
            }
            Self::ImportCeiling | Self::ExportGuard => {
                "measured settled flow within the hard bound"
            }
            Self::FaultBackoff | Self::NoCommand | Self::Ordinary => "no safety command to verify",
            _ => "echo plus two fresh measured neutral reports",
        }
    }

    /// Escalation when verification fails.
    pub fn escalation(self) -> &'static str {
        match self {
            Self::FaultBackoff | Self::NoCommand | Self::Ordinary => "none; named outcome only",
            Self::RestartGrace => "restart_neutral_unverified then ownership_retention",
            Self::CellEmergency => "emergency_charge_unresolved then ownership_retention",
            _ => "retry 0/2/6 s then safety_write_unverified + ownership_retention",
        }
    }
}
