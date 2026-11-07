//! The daily accounting row users see.

/// One `Europe/Amsterdam` day of measured energy and money.
///
/// `actual_eur` is what the day cost with the battery, `baseline_eur` the no-battery
/// counterfactual, and `saving_eur = baseline - actual`. The saving is split into four parts that
/// sum back to it exactly: battery arbitrage, PV self-consumption, the feed-in effect and the
/// battery's own energy balance (see `docs/SAVINGS.md`).
///
/// Money is `None` for a **closed** day whose measured slots were not all present: a day the meter
/// could not cover is stored with `NULL` money and its `coverage_hours`, never with a made-up number.
/// The current day is provisional: it keeps its measured-so-far money and `complete = false` until
/// the day closes. `complete` means every slot of the local day had a price and an import, export,
/// PV and load measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct SavingsDaily {
    pub day_local: String,
    pub actual_eur: Option<f64>,
    pub baseline_eur: Option<f64>,
    pub saving_eur: Option<f64>,
    pub arbitrage_eur: Option<f64>,
    pub self_consumption_eur: Option<f64>,
    pub feed_in_eur: Option<f64>,
    pub battery_balance_eur: Option<f64>,
    pub import_kwh: f64,
    pub export_kwh: f64,
    pub pv_kwh: f64,
    pub battery_charge_kwh: f64,
    pub battery_discharge_kwh: f64,
    pub coverage_hours: f64,
    pub complete: bool,
}

impl SavingsDaily {
    /// A day with money and no energy flows yet; `saving_eur` already equals `baseline - actual`.
    pub fn new(day_local: impl Into<String>, actual_eur: f64, baseline_eur: f64) -> Self {
        Self {
            day_local: day_local.into(),
            actual_eur: Some(actual_eur),
            baseline_eur: Some(baseline_eur),
            saving_eur: Some(baseline_eur - actual_eur),
            arbitrage_eur: None,
            self_consumption_eur: None,
            feed_in_eur: None,
            battery_balance_eur: None,
            import_kwh: 0.0,
            export_kwh: 0.0,
            pv_kwh: 0.0,
            battery_charge_kwh: 0.0,
            battery_discharge_kwh: 0.0,
            coverage_hours: 0.0,
            complete: false,
        }
    }

    /// A closed day the measurements did not cover: `NULL` money, measured energy and coverage kept.
    pub fn incomplete(day_local: impl Into<String>) -> Self {
        let mut day = Self::new(day_local, 0.0, 0.0);
        day.actual_eur = None;
        day.baseline_eur = None;
        day.saving_eur = None;
        day
    }

    /// Whether this row carries money (a complete or provisional day, not a measured gap).
    pub const fn has_money(&self) -> bool {
        self.saving_eur.is_some()
    }

    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let complete: i64 = row.get(14)?;
        Ok(Self {
            day_local: row.get(0)?,
            actual_eur: row.get(1)?,
            baseline_eur: row.get(2)?,
            saving_eur: row.get(3)?,
            arbitrage_eur: row.get(4)?,
            self_consumption_eur: row.get(5)?,
            feed_in_eur: row.get(6)?,
            battery_balance_eur: row.get(7)?,
            import_kwh: row.get(8)?,
            export_kwh: row.get(9)?,
            pv_kwh: row.get(10)?,
            battery_charge_kwh: row.get(11)?,
            battery_discharge_kwh: row.get(12)?,
            coverage_hours: row.get(13)?,
            complete: complete != 0,
        })
    }
}

/// Money is stored rounded to 1e-6 so a re-import of the same day is byte-identical.
pub(crate) fn round_eur(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

/// Round the money columns; an unknown amount stays `NULL`.
pub(crate) fn round_opt(value: Option<f64>) -> Option<f64> {
    value.map(round_eur)
}
