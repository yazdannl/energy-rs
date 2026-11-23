//! Hard physical and site bounds; floor above ceiling is refused before any solve.

use crate::plan_assemble::AssembleRefusal;

#[derive(Debug, Clone, PartialEq)]
pub struct Bounds {
    /// Hard reserve floor, kWh.
    pub floor_kwh: f64,
    /// Usable ceiling, kWh.
    pub ceiling_kwh: f64,
    /// Site import ceiling after margin, W.
    pub import_ceiling_w: f64,
    /// Site export ceiling magnitude, W.
    pub export_ceiling_w: f64,
    /// Aggregate pack charge limit, W.
    pub charge_limit_w: f64,
    /// Aggregate pack discharge limit, W.
    pub discharge_limit_w: f64,
    /// Charge efficiency, stored per AC kWh.
    pub eta_charge: f64,
    /// Discharge efficiency, AC per stored kWh.
    pub eta_discharge: f64,
    /// Slot length in seconds.
    pub slot_seconds: i64,
}

impl Bounds {
    /// Validate bounds and physical constants before a plan is accepted.
    pub fn validate(&self) -> Result<(), AssembleRefusal> {
        let invalid = !self.floor_kwh.is_finite()
            || !self.ceiling_kwh.is_finite()
            || !self.import_ceiling_w.is_finite()
            || !self.export_ceiling_w.is_finite()
            || !self.charge_limit_w.is_finite()
            || !self.discharge_limit_w.is_finite()
            || !self.eta_charge.is_finite()
            || !self.eta_discharge.is_finite()
            || self.floor_kwh > self.ceiling_kwh
            || self.import_ceiling_w < 0.0
            || self.export_ceiling_w < 0.0
            || self.charge_limit_w < 0.0
            || self.discharge_limit_w < 0.0
            || !(0.0..=1.0).contains(&self.eta_charge)
            || !(0.0..=1.0).contains(&self.eta_discharge)
            || self.eta_charge == 0.0
            || self.eta_discharge == 0.0
            || self.slot_seconds <= 0;
        if invalid {
            return Err(AssembleRefusal::InvalidBounds {
                floor_kwh: self.floor_kwh,
                ceiling_kwh: self.ceiling_kwh,
            });
        }
        Ok(())
    }
}

/// Live fleet constants: learned capacity, 15 percent floor and 7500 W effective import ceiling.
pub fn fleet_bounds() -> Bounds {
    Bounds {
        floor_kwh: crate::HARD_FLOOR_KWH,
        ceiling_kwh: crate::LEARNED_CAPACITY_KWH,
        import_ceiling_w: crate::IMPORT_CEILING_W,
        export_ceiling_w: crate::FLEET_POWER_W,
        charge_limit_w: crate::FLEET_POWER_W,
        discharge_limit_w: crate::FLEET_POWER_W,
        eta_charge: 0.9,
        eta_discharge: 0.9,
        slot_seconds: crate::SLOT_SECONDS,
    }
}
