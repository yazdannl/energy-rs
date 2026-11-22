//! The solar-reserve bound of the safety ladder.
//!
//! The reserve protects forecast PV headroom. It is released only by the planner's forecast
//! verdict (`solar_reserve_armed`), never by a full pack, an export limit, or a momentary PV drop.
//! A cell latch or an unavailable device can make forecast surplus unusable, but those are handled
//! by the existing per-pack guards; they are not reasons to weaken this reserve rule.

use super::{Reason, ladder::Ladder};

impl Ladder<'_> {
    /// Bound grid charging above the solar ceiling by the measured PV surplus.
    ///
    /// A due seven-day balance is exempt from this bound only: the device limit, physical SoC
    /// ceiling, plan trajectory and grid/import ceilings still apply. When the reserve is armed and
    /// the pack is above the threshold, missing or non-finite PV/load telemetry blocks the charge
    /// with a named reason instead of silently consuming the reserve.
    pub(crate) fn solar_ceiling(&mut self) {
        if self.request.maintenance_full_charge
            || !self.request.solar_reserve_armed
            || self.limits.solar_reserve_pct <= 0.0
        {
            return;
        }
        let Some(slot) = self.request.plan_slot.as_ref() else {
            return;
        };
        if slot.battery_kw <= super::ladder::NEGLIGIBLE_KW {
            return;
        }
        let ceiling =
            self.pack.e_max_kwh() - self.pack.capacity_kwh * self.limits.solar_reserve_pct / 100.0;
        if self.energy_kwh < ceiling - super::ladder::SOLAR_HYSTERESIS_KWH {
            return;
        }
        let pv = self
            .request
            .measured
            .pv_kw
            .filter(|value| value.is_finite());
        let load = self
            .request
            .measured
            .load_kw
            .filter(|value| value.is_finite());
        if pv.is_none() || load.is_none() {
            let reason = if pv.is_none() {
                Reason::SolarPvTelemetryMissing
            } else {
                Reason::SolarLoadTelemetryMissing
            };
            self.solar_missing = Some(reason);
            self.soc_hi = 0.0;
            return;
        }
        let (Some(pv), Some(load)) = (pv, load) else {
            return;
        };
        let surplus = ((pv - load) * 1000.0).max(0.0) * self.share();
        if surplus < slot.battery_kw * 1000.0 {
            self.soc_hi = self.soc_hi.min(surplus);
        }
    }
}
