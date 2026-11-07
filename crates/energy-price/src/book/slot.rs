//! One settlement slot: the published all-in price and the market price derived from it once, at
//! ingest.  Downstream code never recomputes `market` from `buy_all_in`; it reads this field.

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;

use crate::book::SlotSource;
use crate::book::query::TARIFF_TZ;

/// Prices are money in EUR/kWh, rounded to 1e-6 on write (PLAN.md §2).
pub const MONEY_SCALE: f64 = 1_000_000.0;

/// A single tariff slot on a contiguous grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    /// UTC epoch seconds of the slot start; must lie on a [`SlotSource`] boundary.
    pub t_start: i64,
    /// EUR/kWh exactly as published: energy tax, VAT and the supplier fee included.
    pub buy_all_in: f64,
    /// EUR/kWh market price, derived at ingest: `(buy_all_in - fee_incl) / (1 + vat) - tax_ex`.
    pub market: f64,
    /// Which provider/resolution produced the slot.
    pub source: SlotSource,
}

impl Slot {
    /// Construct a slot from values already derived (providers use this through the book builder).
    pub const fn new(t_start: i64, buy_all_in: f64, market: f64, source: SlotSource) -> Self {
        Self {
            t_start,
            buy_all_in,
            market,
            source,
        }
    }

    /// Exclusive end of the slot, in UTC epoch seconds.
    pub const fn t_end(&self) -> i64 {
        self.t_start + self.source.slot_seconds()
    }

    /// Start instant in the tariff timezone (`Europe/Amsterdam`) when representable.
    pub fn local_start(&self) -> Option<DateTime<Tz>> {
        DateTime::<Utc>::from_timestamp(self.t_start, 0).map(|utc| utc.with_timezone(&TARIFF_TZ))
    }

    /// Local tariff date (the key for saldering years, DST and publication timing).
    pub fn local_date(&self) -> Option<NaiveDate> {
        self.local_start().map(|start| start.date_naive())
    }

    /// Whether both money fields are finite.
    pub fn is_finite(&self) -> bool {
        self.buy_all_in.is_finite() && self.market.is_finite()
    }

    /// Round the money fields to 1e-6 EUR/kWh.
    pub fn money_rounded(self) -> Self {
        Self {
            buy_all_in: round_money(self.buy_all_in),
            market: round_money(self.market),
            ..self
        }
    }
}

/// Round a EUR/kWh amount to the store's 1e-6 resolution.
pub fn round_money(value: f64) -> f64 {
    if value.is_finite() {
        (value * MONEY_SCALE).round() / MONEY_SCALE
    } else {
        value
    }
}
