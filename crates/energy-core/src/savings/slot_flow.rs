//! One quarter-hour of measured house flows, as the Home Assistant energy dashboard reports them.

/// A derived load this far below zero is statistics rounding, not a broken balance (kWh).
///
/// The dashboard's `change` values are independent sensor accumulators; a few Wh of disagreement is
/// noise. A larger negative load means a counter is missing or broken, and the slot is a gap.
const LOAD_TOLERANCE_KWH: f64 = 0.005;

/// One 15-minute slot of measured energy, kWh.
///
/// Every field is an `Option`: `None` is a gap in the statistic, never a zero. The load follows the
/// dashboard's own balance `import - export + pv + discharge - charge` and is `None` as soon as one
/// of those inputs is missing. A dashboard without a battery reports `Some(0.0)` for both battery
/// counters; a configured counter with a hole reports `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotFlow {
    pub t_start: i64,
    pub import_kwh: Option<f64>,
    pub export_kwh: Option<f64>,
    pub pv_kwh: Option<f64>,
    pub charge_kwh: Option<f64>,
    pub discharge_kwh: Option<f64>,
}

impl SlotFlow {
    /// A slot with every measurement missing.
    pub const fn empty(t_start: i64) -> Self {
        Self {
            t_start,
            import_kwh: None,
            export_kwh: None,
            pv_kwh: None,
            charge_kwh: None,
            discharge_kwh: None,
        }
    }

    /// A fully measured slot.
    pub const fn measured(
        t_start: i64,
        import_kwh: f64,
        export_kwh: f64,
        pv_kwh: f64,
        charge_kwh: f64,
        discharge_kwh: f64,
    ) -> Self {
        Self {
            t_start,
            import_kwh: Some(import_kwh),
            export_kwh: Some(export_kwh),
            pv_kwh: Some(pv_kwh),
            charge_kwh: Some(charge_kwh),
            discharge_kwh: Some(discharge_kwh),
        }
    }

    /// The house load from the dashboard balance, or `None` when an input is missing.
    pub fn load_kwh(&self) -> Option<f64> {
        Some(
            self.import_kwh? - self.export_kwh? + self.pv_kwh? + self.discharge_kwh?
                - self.charge_kwh?,
        )
    }

    /// Whether this slot has a usable import, export, PV and load measurement.
    ///
    /// A load materially below zero (or any non-finite input) is an inconsistent reading — a counter
    /// that does not add up — so the slot is a gap and the day cannot claim it. A few Wh of
    /// disagreement is statistics noise and is kept as-is; the counterfactual stays exact either way
    /// because it is derived from the same balance.
    pub fn is_measured(&self) -> bool {
        self.load_kwh()
            .is_some_and(|load| load.is_finite() && load >= -LOAD_TOLERANCE_KWH)
    }
}
