//! The post-net-metering dispatch policy: export gate, reserves, and their knobs.
//!
//! The numbers here are the ones in `docs/POLICY.md`; this module only holds them and the formulas
//! that turn them into solver inputs. It performs no I/O and knows nothing about Home Assistant.

use crate::plan::ExportPolicy;

/// The policy in force for one plan.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Policy {
    /// Export rule.
    pub export: ExportPolicy,
    /// Headroom kept free of grid energy, percent of capacity.
    pub solar_reserve_pct: f64,
    /// Energy worth keeping for a peak the forecast missed, percent of capacity.
    pub peak_reserve_pct: f64,
    /// What a kWh of peak headroom is worth, EUR/kWh (the soft target's penalty).
    pub peak_reserve_value_eur_per_kwh: f64,
    /// Wear per delivered AC kWh, EUR/kWh.
    pub wear_eur_per_kwh: f64,
    /// Average cost of the energy in the pack, EUR per AC kWh; `None` when unknown.
    pub cost_basis_eur_per_kwh: Option<f64>,
}

impl Policy {
    /// The shipped policy: no export gate, no reserves, no wear (today's behaviour).
    pub fn legacy() -> Self {
        Self::default()
    }

    /// The break-even feed-in for an exported kWh: `b / η_rt + w`.
    ///
    /// One exported AC kWh empties `1/η_d` DC, which costs `1/(η_c·η_d)` AC to replace. `None` when
    /// no basis is known: the caller then refuses export rather than guessing.
    pub fn break_even(&self, eta_rt: f64) -> Option<f64> {
        let basis = self.cost_basis_eur_per_kwh?;
        if !basis.is_finite() || basis < 0.0 || !eta_rt.is_finite() || eta_rt <= 0.0 {
            return None;
        }
        let break_even = basis / eta_rt + self.wear_eur_per_kwh.max(0.0);
        break_even.is_finite().then_some(break_even)
    }

    /// The soft peak guideline in kWh: `min(reserve, stored)`, never unreachable.
    pub fn peak_target_kwh(&self, capacity_kwh: f64, stored_kwh: f64) -> Option<f64> {
        let reserve = self.peak_reserve_pct.clamp(0.0, 100.0) / 100.0 * capacity_kwh;
        (reserve > 0.0).then(|| reserve.min(stored_kwh.max(0.0)))
    }

    /// The solar reserve in kWh: headroom kept free of grid energy.
    pub fn solar_reserve_kwh(&self, capacity_kwh: f64) -> f64 {
        (self.solar_reserve_pct.clamp(0.0, 100.0) / 100.0 * capacity_kwh).max(0.0)
    }
}
