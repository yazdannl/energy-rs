//! One 15-minute slot of a plan.

/// A plan slot: what the optimizer wanted at `t_start`, in the units of the optimizer (kW for
/// power, kWh for SoC) and EUR/kWh for prices. `battery_kw` is positive for charging and negative
/// for discharging; `reason` is the human-readable rationale shown in the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanSlot {
    pub plan_id: i64,
    pub idx: i64,
    pub t_start: i64,
    pub buy_all_in: f64,
    pub feed_in: f64,
    pub pv_kw: f64,
    pub load_kw: f64,
    pub battery_kw: f64,
    pub soc_kwh: f64,
    pub target_soc_kwh: f64,
    pub reason: String,
}

impl PlanSlot {
    /// An all-zero slot at `idx`/`t_start`; the caller fills in the optimiser's numbers.
    pub fn at(idx: i64, t_start: i64) -> Self {
        Self {
            plan_id: 0,
            idx,
            t_start,
            buy_all_in: 0.0,
            feed_in: 0.0,
            pv_kw: 0.0,
            load_kw: 0.0,
            battery_kw: 0.0,
            soc_kwh: 0.0,
            target_soc_kwh: 0.0,
            reason: String::new(),
        }
    }

    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            plan_id: row.get(0)?,
            idx: row.get(1)?,
            t_start: row.get(2)?,
            buy_all_in: row.get(3)?,
            feed_in: row.get(4)?,
            pv_kw: row.get(5)?,
            load_kw: row.get(6)?,
            battery_kw: row.get(7)?,
            soc_kwh: row.get(8)?,
            target_soc_kwh: row.get(9)?,
            reason: row.get(10)?,
        })
    }
}
