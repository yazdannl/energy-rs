//! The ordered bounds, one field each, combined once into the decision.

use energy_battery::protection::CellLatch;
use energy_ledger::plan::PlanSlot;

use crate::plan::{PackSpec, SLOT_MINUTES, SLOT_SECONDS};
use crate::safety::decision::SafetyDecision;
use crate::safety::{CommandRequest, Reason, SafetyLimits};

const NEGLIGIBLE_KW: f64 = 1e-9;

/// The bounds collected in ladder order, then applied together.
#[allow(clippy::struct_excessive_bools)] // one boolean per independent bound
pub(crate) struct Ladder<'a> {
    pub(crate) request: &'a CommandRequest,
    pub(crate) pack: &'a PackSpec,
    pub(crate) limits: &'a SafetyLimits,
    pub(crate) target: f64,
    pub(crate) energy_kwh: f64,
    pub(crate) dt_hours: f64,
    pub(crate) p_max: f64,
    pub(crate) p_min: f64,
    pub(crate) soc_lo: f64,
    pub(crate) soc_hi: f64,
    pub(crate) track_lo: Option<f64>,
    pub(crate) track_hi: Option<f64>,
    pub(crate) grid_lo: f64,
    pub(crate) grid_hi: f64,
    pub(crate) grid_import: bool,
    pub(crate) grid_export: bool,
    pub(crate) grid_missing: bool,
    pub(crate) base_kw: Option<f64>,
    pub(crate) ramp_lo: f64,
    pub(crate) ramp_hi: f64,
    pub(crate) floor_lo: Option<f64>,
    pub(crate) latch_lo: Option<f64>,
    pub(crate) forced_charge: bool,
}

impl<'a> Ladder<'a> {
    pub(crate) fn new(
        request: &'a CommandRequest,
        pack: &'a PackSpec,
        limits: &'a SafetyLimits,
        energy_kwh: f64,
        latch: Option<&CellLatch>,
    ) -> Self {
        let forced = latch.filter(|item| item.force_charge);
        let forced_charge = forced.is_some();
        let target = forced.map_or(request.requested_w, |item| {
            let watts = item.force_charge_w.map_or(pack.max_charge_w, f64::from);
            request.requested_w.max(watts)
        });
        let p_max = cap(pack.max_charge_w, limits.fleet_max_charge_w);
        let p_min = -cap(pack.max_discharge_w, limits.fleet_max_discharge_w);
        Self {
            request,
            pack,
            limits,
            target,
            energy_kwh,
            dt_hours: limits.tick_seconds.max(1.0) / 3600.0,
            p_max,
            p_min,
            soc_lo: p_min,
            soc_hi: p_max,
            track_lo: None,
            track_hi: None,
            grid_lo: p_min,
            grid_hi: p_max,
            grid_import: false,
            grid_export: false,
            grid_missing: false,
            base_kw: None,
            ramp_lo: p_min,
            ramp_hi: p_max,
            floor_lo: None,
            latch_lo: latch.filter(|item| item.blocks_discharge()).map(|_| 0.0),
            forced_charge,
        }
    }

    /// Run the ladder in its documented order and combine the bounds.
    pub(crate) fn run(mut self) -> SafetyDecision {
        self.soc(self.energy_kwh);
        self.track();
        self.grid();
        self.ramp();
        self.floor();
        self.finish()
    }

    /// Pack SoC ceiling and floor, with the reserve applied to this pack's own capacity.
    fn soc(&mut self, energy_kwh: f64) {
        let eta_charge = self.pack.eta_charge;
        let eta_discharge = self.pack.eta_discharge;
        let room = (self.pack.e_max_kwh() - energy_kwh).max(0.0);
        let headroom = (energy_kwh - self.pack.e_min_kwh(self.limits.reserve_soc_pct)).max(0.0);
        let hi = room / (self.dt_hours * eta_charge) * 1000.0;
        let lo = -headroom * eta_discharge / self.dt_hours * 1000.0;
        self.soc_hi = self.p_max.min(hi);
        self.soc_lo = self.p_min.max(lo);
    }

    /// Land the pack on the plan's end energy, never reversing the plan's direction.
    fn track(&mut self) {
        let Some(slot) = self.request.plan_slot.as_ref() else {
            return;
        };
        if slot.battery_kw.abs() <= NEGLIGIBLE_KW {
            return;
        }
        let target = self.track_target(slot);
        let remaining = self.remaining_hours(slot);
        if slot.battery_kw > 0.0 {
            let watts =
                (target - self.energy_kwh).max(0.0) / (remaining * self.pack.eta_charge) * 1000.0;
            self.track_hi = Some(watts.max(0.0));
        } else {
            let watts = (target - self.energy_kwh) * self.pack.eta_discharge / remaining * 1000.0;
            self.track_lo = Some(watts.min(0.0));
        }
    }

    /// The pack's share of the plan's end energy for this slot.
    fn track_target(&self, slot: &PlanSlot) -> f64 {
        let hours = SLOT_MINUTES / 60.0;
        let end = if slot.battery_kw > 0.0 {
            slot.soc_kwh + slot.battery_kw * hours * self.pack.eta_charge
        } else {
            slot.soc_kwh + slot.battery_kw * hours / self.pack.eta_discharge
        };
        end * self.share()
    }

    fn share(&self) -> f64 {
        if self.limits.fleet_capacity_kwh > 0.0 {
            (self.pack.capacity_kwh / self.limits.fleet_capacity_kwh).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    fn remaining_hours(&self, slot: &PlanSlot) -> f64 {
        let end = slot.t_start.saturating_add(SLOT_SECONDS);
        let seconds = end.saturating_sub(self.request.now) as f64 / 3600.0;
        if seconds <= 0.0 {
            self.dt_hours
        } else {
            seconds
        }
    }

    /// Grid import/export bounds, corrected back to the battery-idle base; never guessed.
    fn grid(&mut self) {
        let plan_base = self
            .request
            .plan_slot
            .as_ref()
            .map(|slot| slot.load_kw - slot.pv_kw);
        let measured = self
            .request
            .measured
            .grid_w
            .map(|watts| watts / 1000.0 - self.request.measured.fleet_power_w / 1000.0);
        let forecast = plan_base.or(
            match (self.request.measured.load_kw, self.request.measured.pv_kw) {
                (Some(load), Some(pv)) => Some(load - pv),
                _ => None,
            },
        );
        self.base_kw = forecast.or(measured);
        let import_base = pick(forecast, measured, f64::max);
        let export_base = pick(forecast, measured, f64::min);
        self.grid_import = self.limits.max_grid_import_w > 0.0;
        self.grid_export = self.limits.max_grid_export_w > 0.0;
        if self.grid_import {
            if let Some(base) = import_base {
                self.grid_hi = self
                    .p_max
                    .min(self.limits.max_grid_import_w - base * 1000.0);
            } else {
                self.grid_missing = true;
            }
        }
        if self.grid_export
            && let Some(base) = export_base
        {
            self.grid_lo = self
                .p_min
                .max(-(self.limits.max_grid_export_w + base * 1000.0));
        } else if self.grid_export {
            self.grid_missing = true;
        }
    }

    fn ramp(&mut self) {
        let limit = self.limits.ramp_limit_w.max(0.0);
        let previous = self.request.measured.power_w;
        self.ramp_lo = previous - limit;
        self.ramp_hi = previous + limit;
    }

    /// The cost-basis floor judges only the plan's own planned discharge.
    fn floor(&mut self) {
        let (Some(floor), Some(slot)) = (
            self.limits.sell_floor_eur_per_kwh,
            self.request.plan_slot.as_ref(),
        ) else {
            return;
        };
        if slot.battery_kw >= -NEGLIGIBLE_KW {
            return;
        }
        let planned_grid = self.base_kw.unwrap_or(0.0) + slot.battery_kw;
        let leg = if planned_grid < 0.0 {
            slot.feed_in
        } else {
            slot.buy_all_in
        };
        if leg < floor {
            self.floor_lo = Some(0.0);
        }
    }
}

fn cap(pack_w: f64, fleet_w: f64) -> f64 {
    if fleet_w > 0.0 {
        pack_w.min(fleet_w)
    } else {
        pack_w
    }
}

fn pick(forecast: Option<f64>, measured: Option<f64>, choose: fn(f64, f64) -> f64) -> Option<f64> {
    match (forecast, measured) {
        (Some(a), Some(b)) => Some(choose(a, b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

pub(crate) fn push_if(reasons: &mut Vec<Reason>, condition: bool, reason: Reason) {
    if condition {
        reasons.push(reason);
    }
}
