//! A bound that bound the plan; clipping is evidence, not by itself an infeasibility.

#[derive(Debug, Clone, PartialEq)]
pub struct BindingBound {
    /// Typed reason slug, for example pv_headroom or soc_floor.
    pub reason: &'static str,
    /// Slot start when the bound is per slot.
    pub slot: Option<i64>,
    /// Bound value in kW or kWh, depending on reason.
    pub kw_or_kwh: f64,
}
