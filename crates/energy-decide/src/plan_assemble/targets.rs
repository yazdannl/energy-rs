//! Effective terminal target from the soft guideline, hard floor and ceiling.

use crate::plan_assemble::AssembleRefusal;

#[derive(Debug, Clone, PartialEq)]
pub struct Targets {
    /// Raw 10 percent soft guideline, kWh.
    pub soft_guideline_kwh: f64,
    /// Hard 15 percent floor, kWh.
    pub hard_floor_kwh: f64,
    /// Usable ceiling, kWh.
    pub ceiling_kwh: f64,
    /// Configured terminal target percentage, when present.
    pub terminal_target_pct: Option<f64>,
    /// Learned pooled capacity used as the planning basis, kWh.
    pub learned_capacity_kwh: f64,
    /// Nameplate capacity, a prior only, kWh.
    pub label_capacity_kwh: f64,
}

impl Targets {
    /// Effective target T, independent of horizon length.
    pub fn effective_kwh(&self) -> f64 {
        self.soft_guideline_kwh
            .max(self.hard_floor_kwh)
            .min(self.ceiling_kwh)
    }

    /// Guideline expressed on the label basis, which is reporting only.
    pub fn label_guideline_kwh(&self) -> f64 {
        self.soft_guideline_kwh / self.learned_capacity_kwh * self.label_capacity_kwh
    }

    /// Reject non-finite/invalid energy inputs before planning.
    pub fn validate_finite(&self) -> Result<(), AssembleRefusal> {
        let finite = self.soft_guideline_kwh.is_finite()
            && self.hard_floor_kwh.is_finite()
            && self.ceiling_kwh.is_finite()
            && self.learned_capacity_kwh.is_finite()
            && self.label_capacity_kwh.is_finite()
            && self.learned_capacity_kwh > 0.0
            && self.label_capacity_kwh > 0.0
            && self.hard_floor_kwh <= self.ceiling_kwh;
        if finite {
            return Ok(());
        }
        Err(AssembleRefusal::InvalidTargets {
            soft_guideline_kwh: self.soft_guideline_kwh,
            hard_floor_kwh: self.hard_floor_kwh,
            ceiling_kwh: self.ceiling_kwh,
        })
    }

    /// Reject a configured percentage outside 0..=100 before planning.
    pub fn validate_pct(&self) -> Result<(), AssembleRefusal> {
        if let Some(pct) = self.terminal_target_pct
            && (!pct.is_finite() || !(0.0..=100.0).contains(&pct))
        {
            return Err(AssembleRefusal::TerminalTargetPct { pct });
        }
        Ok(())
    }
}

/// The live learned-basis target pair used by the fleet.
pub fn fleet_targets() -> Targets {
    Targets {
        soft_guideline_kwh: crate::SOFT_GUIDELINE_KWH,
        hard_floor_kwh: crate::HARD_FLOOR_KWH,
        ceiling_kwh: crate::LEARNED_CAPACITY_KWH,
        terminal_target_pct: None,
        learned_capacity_kwh: crate::LEARNED_CAPACITY_KWH,
        label_capacity_kwh: crate::LABEL_CAPACITY_KWH,
    }
}
