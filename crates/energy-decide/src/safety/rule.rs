//! The one ordered override list. Solar headroom is deliberately not in it.

/// Ordered safety rules; the first firing command or deferral wins, every firing reason is reported.
///
/// The physical service limit is unconditional and outranks the economic import ceiling;
/// emergency charging may only breach the 200 W margin, never the service fuse. The cumulative
/// observation-budget rule replaces any per-sample age check for nonzero commands. `Ordinary`
/// is the explicit no-action outcome, never an empty tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    /// Latched cell discharge: verified 0 W.
    CellLatch,
    /// Latched low cell: the minimum bounded charge, or a 0 W hold fallback.
    CellEmergency,
    /// Hard SoC/minSoc floor: no discharge.
    SocFloor,
    /// Unconditional physical service fuse; unknown headroom fails closed.
    PhysicalServiceLimit,
    /// Projected import breach: protective discharge or charge reduction.
    ImportCeiling,
    /// SoC ceiling / full-charge protection: no charge, no fault escalation.
    FullCharge,
    /// Export/self-consumption hard guard.
    ExportGuard,
    /// Samples older than the 300 s reachability TTL: unreachable until neutral.
    StaleSample,
    /// Cumulative unobserved-time budget exhausted: no nonzero command.
    ObservationBudget,
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
pub const RULE_ORDER: [Rule; 17] = [
    Rule::CellLatch,
    Rule::CellEmergency,
    Rule::SocFloor,
    Rule::PhysicalServiceLimit,
    Rule::ImportCeiling,
    Rule::FullCharge,
    Rule::ExportGuard,
    Rule::StaleSample,
    Rule::ObservationBudget,
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
            Self::PhysicalServiceLimit => "physical_service_limit",
            Self::ImportCeiling => "grid_import_ceiling",
            Self::FullCharge => "full_charge_protection",
            Self::ExportGuard => "self_consumption_guard",
            Self::StaleSample => "stale_sample_unreachable",
            Self::ObservationBudget => "observation_budget_exhausted",
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
            Self::PhysicalServiceLimit => {
                "protective reduction if headroom is known, else 0 W hold"
            }
            Self::CellEmergency => "bounded minimum charge, else 0 W verified hold",
            Self::ImportCeiling => "protective discharge/charge reduction, else 0 W hold",
            Self::UncertainWrite => "0 W hold until neutrality is proven",
            Self::FaultBackoff => "no command; defer ordinary write to retry_at",
            Self::NoCommand | Self::Ordinary => "no safety command",
            _ => "0 W verified neutral hold",
        }
    }

    /// Verification requirement for the rule's command.
    pub fn verification(self) -> &'static str {
        match self {
            Self::CellEmergency => {
                "echo plus cell voltage at resume threshold within 180 s; else verified hold"
            }
            Self::PhysicalServiceLimit | Self::ImportCeiling | Self::ExportGuard => {
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
