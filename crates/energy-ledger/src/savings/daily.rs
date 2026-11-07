//! The daily accounting row users see.

/// One `Europe/Amsterdam` day of measured energy and money.
///
/// `actual_eur` is what the day cost with the battery, `baseline_eur` the no-battery
/// counterfactual, and `saving_eur = baseline - actual`. `coverage_hours` records how much of the
/// day had measured data; a day with a missing interval is stored with lower coverage instead of
/// being dropped (the old add-on's "one missing interval nulls the day" bug). `complete` means the
/// full 24 h were covered.
#[derive(Debug, Clone, PartialEq)]
pub struct SavingsDaily {
    pub day_local: String,
    pub actual_eur: f64,
    pub baseline_eur: f64,
    pub saving_eur: f64,
    pub import_kwh: f64,
    pub export_kwh: f64,
    pub pv_kwh: f64,
    pub battery_charge_kwh: f64,
    pub battery_discharge_kwh: f64,
    pub coverage_hours: f64,
    pub complete: bool,
}

impl SavingsDaily {
    /// A day with no energy flows yet; `saving_eur` already equals `baseline - actual`.
    pub fn new(day_local: impl Into<String>, actual_eur: f64, baseline_eur: f64) -> Self {
        Self {
            day_local: day_local.into(),
            actual_eur,
            baseline_eur,
            saving_eur: baseline_eur - actual_eur,
            import_kwh: 0.0,
            export_kwh: 0.0,
            pv_kwh: 0.0,
            battery_charge_kwh: 0.0,
            battery_discharge_kwh: 0.0,
            coverage_hours: 0.0,
            complete: false,
        }
    }

    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let complete: i64 = row.get(10)?;
        Ok(Self {
            day_local: row.get(0)?,
            actual_eur: row.get(1)?,
            baseline_eur: row.get(2)?,
            saving_eur: row.get(3)?,
            import_kwh: row.get(4)?,
            export_kwh: row.get(5)?,
            pv_kwh: row.get(6)?,
            battery_charge_kwh: row.get(7)?,
            battery_discharge_kwh: row.get(8)?,
            coverage_hours: row.get(9)?,
            complete: complete != 0,
        })
    }
}

/// Money is stored rounded to 1e-6 so a re-import of the same day is byte-identical.
pub(crate) fn round_eur(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}
