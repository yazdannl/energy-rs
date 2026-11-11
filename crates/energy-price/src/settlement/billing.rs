//! The single money authority.  `buy`, `export_price` and `revenue` are pure and are what the
//! optimizer, the dispatcher, the sell floor and reporting call; `settle` additionally advances the
//! saldering budget and the Zonnebonus counter after a slot has actually been settled.

use chrono::{Datelike, NaiveDate, Utc};

use crate::book::Slot;
use crate::book::query::{TARIFF_TZ, local_date_of, local_year_of};
use crate::settlement::{Contract, ExportSource, NetMetering, Saldering};

/// What "no export budget tracked" answers: effectively infinite netting, but finite.
const UNTRACKED_BUDGET_KWH: f64 = 1.0e9;

/// Contract, regime, budget and bonus counters: everything needed to price a slot.
#[derive(Debug, Clone, PartialEq)]
pub struct Settlement {
    contract: Contract,
    regime: NetMetering,
    budget: Option<Saldering>,
    bonus_ytd_kwh: f64,
    bonus_year: i32,
    today: NaiveDate,
}

impl Settlement {
    /// Date-driven regime with the system clock's Amsterdam date; no saldering budget yet.
    pub fn new(contract: Contract) -> Self {
        let today = Utc::now().with_timezone(&TARIFF_TZ).date_naive();
        Self::on(contract, today)
    }

    /// Deterministic constructor: the regime falls back to `today` for slots without a local date.
    pub fn on(contract: Contract, today: NaiveDate) -> Self {
        Self {
            contract,
            regime: NetMetering::On,
            budget: None,
            bonus_ytd_kwh: 0.0,
            bonus_year: today.year(),
            today,
        }
    }

    /// The contract constants in force.
    pub fn contract(&self) -> &Contract {
        &self.contract
    }

    /// The regime switch (date-driven by default).
    pub fn regime(&self) -> NetMetering {
        self.regime
    }

    /// The annual saldering budget, when configured.
    pub fn budget(&self) -> Option<&Saldering> {
        self.budget.as_ref()
    }

    /// Bonus-earning export counted against the Zonnebonus cap, kWh for [`Settlement::bonus_year`].
    pub fn bonus_ytd_kwh(&self) -> f64 {
        self.bonus_ytd_kwh
    }

    /// Calendar year the Zonnebonus counter belongs to; the cap is annual, not lifetime.
    pub fn bonus_year(&self) -> i32 {
        self.bonus_year
    }

    /// Local date that drives the date-driven regime.
    pub fn today(&self) -> NaiveDate {
        self.today
    }

    /// Manual override of the regime (legacy netting contracts).
    pub fn set_regime(&mut self, regime: NetMetering) {
        self.regime = regime;
    }

    /// Move the date-driven regime to a new local date.
    pub fn set_today(&mut self, today: NaiveDate) {
        self.today = today;
    }

    /// Replace or clear the saldering budget.
    pub fn set_budget(&mut self, budget: Option<Saldering>) {
        self.budget = budget;
    }

    /// Install a utility-meter import reading for `year` (reset first on a new year).
    pub fn set_import_ytd(&mut self, year: i32, kwh: f64) {
        match self.budget.as_mut() {
            Some(budget) => {
                if budget.year != year {
                    budget.reset(year);
                }
                budget.record_import(kwh);
            }
            None => self.budget = Some(Saldering::new(kwh, 0.0, year)),
        }
    }

    /// Restore the persisted bonus counter for its year; other years count from zero.
    pub fn set_bonus_ytd_kwh(&mut self, year: i32, kwh: f64) {
        self.bonus_year = year;
        self.bonus_ytd_kwh = if kwh.is_finite() { kwh.max(0.0) } else { 0.0 };
    }

    /// Whether 1:1 netting applies to this slot's tariff date.
    ///
    /// With the regime on and **no budget tracked**, netting is uncapped: a household that never
    /// told us its annual consumption still nets every exported kWh 1:1 against an import, and
    /// pricing its exports at the post-saldering feed-in would make the plan refuse to export at
    /// all. An exhausted (or absent) budget used to mean "netting off" for every slot, which is how
    /// the app ended up showing the 2027 price in 2026.
    pub fn saldering_active(&self, slot: &Slot) -> bool {
        self.regime.is_active(self.slot_date(slot)) && self.budget_remaining(slot) > 0.0
    }

    /// kWh of export still netted 1:1 in this slot's tariff year.
    pub fn budget_remaining(&self, slot: &Slot) -> f64 {
        let date = self.slot_date(slot);
        if !self.regime.is_active(date) {
            return 0.0;
        }
        match self.budget {
            Some(budget) => budget.budget_remaining(date.year()),
            // Uncapped: finite on purpose, so no product of this can turn into a NaN downstream.
            None => UNTRACKED_BUDGET_KWH,
        }
    }

    /// The published all-in import price, EUR/kWh.
    ///
    /// A kWh discharged into the house replaces an import and is worth exactly this, whatever the
    /// netting regime says about exports.
    pub fn buy(&self, slot: &Slot) -> f64 {
        slot.buy_all_in
    }

    /// Raw post-saldering feed-in price (may be negative; export uses the clamped price).
    pub fn feed_in_post(&self, slot: &Slot, source: ExportSource) -> f64 {
        self.contract
            .feed_in_post(slot, source, self.bonus_kwh_for(slot))
    }

    /// The export price per kWh: 1:1 all-in while budget remains, else the clamped feed-in.
    pub fn export_price(&self, slot: &Slot, source: ExportSource) -> f64 {
        if self.budget_remaining(slot) > 0.0 {
            return slot.buy_all_in.max(0.0);
        }
        self.feed_in_post(slot, source).max(0.0)
    }

    /// Marginal value of one exported kWh, EUR/kWh -- the sell floor's input.
    ///
    /// While the saldering budget lasts, an exported kWh spends allowance that a later PV kWh would
    /// otherwise have netted 1:1, so its marginal value is the all-in price minus the export price a
    /// later export earns (`buy - feed_in_post`, ~0.12 in 2026), never the full all-in price
    /// (prices.md §3.2).  Outside saldering it is the export price itself.
    pub fn marginal_export(&self, slot: &Slot, source: ExportSource) -> f64 {
        if self.budget_remaining(slot) > 0.0 {
            (slot.buy_all_in - self.feed_in_post(slot, source)).max(0.0)
        } else {
            self.export_price(slot, source)
        }
    }

    /// Revenue in EUR for a signed net slot flow: positive kWh = export, negative = import.
    ///
    /// Netting is per slot (prices.md §3.2): while budget remains, the netted part earns the all-in
    /// price and only the remainder earns the feed-in price.  Negative import prices pay you;
    /// negative export prices are clamped to zero (export is refused, never charged).
    pub fn revenue(&self, slot: &Slot, kwh: f64, source: ExportSource) -> f64 {
        if kwh < 0.0 {
            return kwh * self.buy(slot);
        }
        let budget = self.budget_remaining(slot);
        if budget > 0.0 {
            let netted = kwh.min(budget);
            let rest = kwh - netted;
            return netted * slot.buy_all_in.max(0.0)
                + rest * self.feed_in_post(slot, source).max(0.0);
        }
        kwh * self.export_price(slot, source)
    }

    /// Settle one slot and advance the budget/bonus counters; returns the same revenue as
    /// [`Settlement::revenue`] would have before the counters moved.
    pub fn settle(&mut self, slot: &Slot, kwh: f64, source: ExportSource) -> f64 {
        let revenue = self.revenue(slot, kwh, source);
        if kwh <= 0.0 {
            return revenue;
        }
        let date = self.slot_date(slot);
        let mut netted = 0.0;
        if self.regime.is_active(date)
            && let Some(budget) = self.budget.as_mut()
        {
            netted = budget.consume(date.year(), kwh);
        }
        let rest = kwh - netted;
        self.roll_bonus_year(slot);
        let counts_for_bonus = rest > 0.0
            && self
                .contract
                .bonus_applies(slot, source, self.bonus_ytd_kwh);
        if counts_for_bonus {
            self.bonus_ytd_kwh += rest;
        }
        revenue
    }

    /// Bonus-earning kWh to compare with the annual cap: zero for another calendar year.
    fn bonus_kwh_for(&self, slot: &Slot) -> f64 {
        match local_year_of(slot.t_start) {
            Some(year) if year == self.bonus_year => self.bonus_ytd_kwh,
            Some(_) => 0.0,
            None => self.bonus_ytd_kwh,
        }
    }

    /// Start a new bonus year when settlement moves forward into one (never backwards).
    fn roll_bonus_year(&mut self, slot: &Slot) {
        if let Some(year) = local_year_of(slot.t_start)
            && year > self.bonus_year
        {
            self.bonus_year = year;
            self.bonus_ytd_kwh = 0.0;
        }
    }

    fn slot_date(&self, slot: &Slot) -> NaiveDate {
        local_date_of(slot.t_start).unwrap_or(self.today)
    }
}
