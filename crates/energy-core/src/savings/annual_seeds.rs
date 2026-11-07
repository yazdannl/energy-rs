//! Year-to-date saldering counters rebuilt from measured daily totals.
//!
//! The budget is `import_ytd - netted_export_ytd`, so a day's seed needs the year's metering up to
//! local midnight before that day. The counters are advanced in day order: a day's imports count
//! first, then its exports consume whatever budget remains. That is a documented approximation —
//! the meter itself nets per slot, but the annual history is read hourly and only the day being
//! accounted is replayed slot by slot.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use energy_price::Saldering;

use super::SettlementSeed;

/// Running counters and the seed of every day already applied, for one calendar year.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SeedState {
    year: i32,
    through: NaiveDate,
    import_ytd_kwh: f64,
    netted_ytd_kwh: f64,
    export_ytd_kwh: f64,
    seeds: BTreeMap<NaiveDate, SettlementSeed>,
}

impl SeedState {
    /// Counters at the start of `year`: zero, with the day before January 1 already applied.
    pub(crate) fn new(year: i32) -> Self {
        let first = NaiveDate::from_ymd_opt(year, 1, 1).unwrap_or(NaiveDate::MIN);
        let through = first.pred_opt().unwrap_or(NaiveDate::MIN);
        Self {
            year,
            through,
            import_ytd_kwh: 0.0,
            netted_ytd_kwh: 0.0,
            export_ytd_kwh: 0.0,
            seeds: BTreeMap::new(),
        }
    }

    /// The calendar year these counters belong to.
    pub(crate) const fn year(&self) -> i32 {
        self.year
    }

    /// First day whose totals have not been applied yet.
    pub(crate) fn next_day(&self) -> NaiveDate {
        self.through.succ_opt().unwrap_or(self.through)
    }

    /// The seed recorded for `day`, when the counters have reached it.
    pub(crate) fn seed(&self, day: NaiveDate) -> Option<SettlementSeed> {
        self.seeds.get(&day).copied()
    }

    /// Record the start-of-day seed, then advance the counters with the day's measured totals.
    pub(crate) fn extend(&mut self, day: NaiveDate, import_kwh: f64, export_kwh: f64) {
        let budget = Saldering::new(self.import_ytd_kwh, self.netted_ytd_kwh, self.year);
        let bonus_ytd_kwh = (self.export_ytd_kwh - self.netted_ytd_kwh).max(0.0);
        self.seeds
            .insert(day, SettlementSeed::new(Some(budget), bonus_ytd_kwh));
        self.import_ytd_kwh += sanitize(import_kwh);
        self.export_ytd_kwh += sanitize(export_kwh);
        let remaining = (self.import_ytd_kwh - self.netted_ytd_kwh).max(0.0);
        self.netted_ytd_kwh += sanitize(export_kwh).min(remaining);
        self.through = day;
    }
}

fn sanitize(kwh: f64) -> f64 {
    if kwh.is_finite() { kwh.max(0.0) } else { 0.0 }
}

/// Group hourly rows into per-local-day `(import_kwh, export_kwh)` totals.
///
/// Missing hours contribute nothing: a recorder gap understates the year-to-date meter, which only
/// ever makes the budget smaller than the utility meter's. It never invents an export.
pub(crate) fn daily_totals(
    import: &BTreeMap<i64, f64>,
    export: &BTreeMap<i64, f64>,
) -> BTreeMap<NaiveDate, (f64, f64)> {
    let mut totals: BTreeMap<NaiveDate, (f64, f64)> = BTreeMap::new();
    for (rows, column) in [(import, 0), (export, 1)] {
        for (t_start, kwh) in rows {
            let Some(day) = energy_price::book::query::local_date_of(*t_start) else {
                continue;
            };
            let entry = totals.entry(day).or_default();
            match column {
                0 => entry.0 += kwh,
                _ => entry.1 += kwh,
            }
        }
    }
    totals
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]

    use super::{SeedState, daily_totals};
    use std::collections::BTreeMap;

    fn day(value: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn imports_grow_the_budget_before_the_day_that_exports_against_it() {
        let mut state = SeedState::new(2026);
        state.extend(day("2026-01-02"), 3.0, 1.0);
        state.extend(day("2026-01-03"), 0.0, 4.0);
        let third = state.seed(day("2026-01-03")).unwrap();
        assert_eq!(third.budget.unwrap().import_ytd_kwh, 3.0);
        // January 2's own import came before its export in the day-order accumulation.
        assert_eq!(third.budget.unwrap().netted_ytd_kwh, 1.0);
        assert_eq!(third.budget.unwrap().budget_remaining(2026), 2.0);
        assert!(third.bonus_ytd_kwh.abs() < 1e-12);

        state.extend(day("2026-01-04"), 1.0, 0.0);
        let fourth = state.seed(day("2026-01-04")).unwrap();
        let budget = fourth.budget.unwrap();
        assert_eq!(budget.import_ytd_kwh, 3.0);
        // The remaining 2 kWh of budget are consumed by January 3's 4 kWh of export.
        assert_eq!(budget.netted_ytd_kwh, 3.0);
        assert_eq!(budget.budget_remaining(2026), 0.0);
        assert!((fourth.bonus_ytd_kwh - 2.0).abs() < 1e-12);
        assert_eq!(state.next_day(), day("2026-01-05"));
    }

    #[test]
    fn hourly_rows_group_into_local_day_totals() {
        let mut import = BTreeMap::new();
        let mut export = BTreeMap::new();
        let start = energy_price::book::query::local_midnight(day("2026-01-02")).unwrap();
        import.insert(start, 1.0);
        import.insert(start + 3600, 2.0);
        export.insert(start + 2 * 3600, 0.5);
        let totals = daily_totals(&import, &export);
        let (imported, exported) = totals[&day("2026-01-02")];
        assert!((imported - 3.0).abs() < 1e-12);
        assert!((exported - 0.5).abs() < 1e-12);
    }
}
