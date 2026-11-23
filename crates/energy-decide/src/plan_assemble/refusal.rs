//! M3 refusals before a plan exists; the timeline is never silently reused.

use thiserror::Error;

use crate::horizon::HorizonRefusal;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum AssembleRefusal {
    #[error("invalid bounds: floor {floor_kwh} kWh, ceiling {ceiling_kwh} kWh")]
    InvalidBounds { floor_kwh: f64, ceiling_kwh: f64 },
    #[error("terminal target percentage {pct} is outside 0..=100")]
    TerminalTargetPct { pct: f64 },
    #[error(
        "invalid terminal target inputs: soft {soft_guideline_kwh}, floor {hard_floor_kwh}, ceiling {ceiling_kwh}"
    )]
    InvalidTargets {
        soft_guideline_kwh: f64,
        hard_floor_kwh: f64,
        ceiling_kwh: f64,
    },
    #[error(transparent)]
    Horizon(#[from] HorizonRefusal),
}
