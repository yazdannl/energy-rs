//! The annual saldering budget: imported kWh so far minus netted export so far, per calendar year.
//! The budget is a persisted scalar imported from the utility meter; only `netted_ytd_kwh` is
//! advanced by settlement.

/// Saldering state for one calendar year.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Saldering {
    /// Imported kWh year to date, from the utility meter.
    pub import_ytd_kwh: f64,
    /// Exported kWh already netted 1:1 this year.
    pub netted_ytd_kwh: f64,
    /// Calendar year this state belongs to (local time).
    pub year: i32,
}

impl Saldering {
    /// Normalise a persisted or configured state (negative kWh cannot exist).
    pub fn new(import_ytd_kwh: f64, netted_ytd_kwh: f64, year: i32) -> Self {
        Self {
            import_ytd_kwh: sanitize(import_ytd_kwh),
            netted_ytd_kwh: sanitize(netted_ytd_kwh),
            year,
        }
    }

    /// kWh of export that can still be netted 1:1 in `year`; zero for any other year.
    pub fn budget_remaining(&self, year: i32) -> f64 {
        if year != self.year {
            return 0.0;
        }
        (self.import_ytd_kwh - self.netted_ytd_kwh).max(0.0)
    }

    /// Advance the import counter (a meter reading for the year).
    pub fn record_import(&mut self, kwh: f64) {
        self.import_ytd_kwh = sanitize(kwh);
    }

    /// Consume budget with `export_kwh`, returning the kWh actually netted 1:1.
    pub fn consume(&mut self, year: i32, export_kwh: f64) -> f64 {
        let budget = self.budget_remaining(year);
        let netted = sanitize(export_kwh).min(budget);
        self.netted_ytd_kwh += netted;
        netted
    }

    /// Start a new calendar year with fresh counters.
    pub fn reset(&mut self, year: i32) {
        self.import_ytd_kwh = 0.0;
        self.netted_ytd_kwh = 0.0;
        self.year = year;
    }
}

fn sanitize(kwh: f64) -> f64 {
    if kwh.is_finite() { kwh.max(0.0) } else { 0.0 }
}
