//! Typed M2 refusals. A refusal never carries an executable schedule.

use thiserror::Error;

use crate::optimizer::BindingBound;

/// Why no economically valid schedule could be emitted.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum OptimizerRefusal {
    #[error("optimizer horizon is empty")]
    EmptyHorizon,
    #[error("optimizer horizon has {slots} slots; 1..=120 accepted")]
    HorizonSlots { slots: usize },
    #[error("slot {slot} has non-finite {field}")]
    NonFiniteInput { slot: usize, field: &'static str },
    #[error("config field {field} is non-finite")]
    NonFiniteConfig { field: &'static str },
    #[error("slot {slot} has negative PV {pv_kw} kW")]
    InvalidPv { slot: usize, pv_kw: f64 },
    #[error("slot {slot} has non-positive duration {dt_s}s")]
    InvalidSlotDuration { slot: usize, dt_s: i64 },
    #[error("inefficiencies must be in (0, 1]: charge {eta_charge}, discharge {eta_discharge}")]
    InvalidEfficiency { eta_charge: f64, eta_discharge: f64 },
    #[error("invalid power limits: charge {charge_w} W, discharge {discharge_w} W")]
    InvalidPower { charge_w: f64, discharge_w: f64 },
    #[error("invalid bounds: floor {floor_kwh} kWh, ceiling {ceiling_kwh} kWh")]
    InvalidBounds { floor_kwh: f64, ceiling_kwh: f64 },
    #[error("start energy {energy_kwh} kWh outside [{floor_kwh}, {ceiling_kwh}] kWh")]
    StartOutsideBounds {
        energy_kwh: f64,
        floor_kwh: f64,
        ceiling_kwh: f64,
    },
    #[error("slot {slot} has no feasible power/curtailment point; bounds: {binding_bounds:?}")]
    Infeasible {
        slot: usize,
        binding_bounds: Vec<BindingBound>,
    },
    /// Retained for M3+ callers; M2 always prices a terminal shortfall softly and never emits it.
    #[error(
        "terminal target {target_kwh} kWh is unreachable: {shortfall_kwh} kWh short at {terminal_kwh} kWh"
    )]
    TerminalTargetUnreachable {
        target_kwh: f64,
        terminal_kwh: f64,
        shortfall_kwh: f64,
        binding_bounds: Vec<BindingBound>,
    },
}

impl OptimizerRefusal {
    /// Stable reason slug that reaches the typed plan refusal.
    pub const fn reason_slug(&self) -> &'static str {
        match self {
            Self::EmptyHorizon => "EmptyHorizon",
            Self::HorizonSlots { .. } => "HorizonSlots",
            Self::NonFiniteInput { .. } => "NonFiniteInput",
            Self::NonFiniteConfig { .. } => "NonFiniteConfig",
            Self::InvalidPv { .. } => "InvalidPv",
            Self::InvalidSlotDuration { .. } => "InvalidSlotDuration",
            Self::InvalidEfficiency { .. } => "InvalidEfficiency",
            Self::InvalidPower { .. } => "InvalidPower",
            Self::InvalidBounds { .. } => "InvalidBounds",
            Self::StartOutsideBounds { .. } => "StartOutsideBounds",
            Self::Infeasible { .. } => "infeasible",
            Self::TerminalTargetUnreachable { .. } => "terminal_target_unreachable",
        }
    }
}
