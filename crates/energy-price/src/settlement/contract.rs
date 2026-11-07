//! The Zonneplan household contract as constants plus the feed-in formula.  Market derivation lives
//! here too, so ingest and settlement read the same numbers.

use crate::book::Slot;
use crate::error::PriceError;
use crate::settlement::{
    BonusRules, Daylight, EnergyTaxTable, ExportSource, TaxYear,
};

/// VAT on energy (prices.md §7 row 4).
pub const VAT_NL: f64 = 0.21;
/// Zonneplan inkoopvergoeding, EUR/kWh incl. VAT (prices.md §7 row 5).
pub const ZONNEPLAN_FEE_INCL: f64 = 0.0200;
/// Zonneplan terugleverkosten, EUR/kWh (prices.md §7 row 7: none, not even after 2026).
pub const ZONNEPLAN_TERUGLEVERKOSTEN: f64 = 0.0;

/// Per-slot derivation and the post-saldering feed-in price.
#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    /// VAT fraction (0.21).
    pub vat: f64,
    /// Supplier fee incl. VAT, EUR/kWh; added to the market price and to the all-in price.
    pub fee_incl: f64,
    /// Fee paid on exported kWh, EUR/kWh (Zonneplan pays its inkoopvergoeding on export).
    pub feed_in_fee: f64,
    /// Per-kWh export charge of the supplier, EUR/kWh (0 for Zonneplan).
    pub teruglever_kosten: f64,
    /// Zonnebonus rules.
    pub bonus: BonusRules,
    /// Site coordinates for the daylight guard; `None` means the guard fails closed.
    pub daylight: Option<Daylight>,
    tax: EnergyTaxTable,
}

impl Contract {
    /// The researched Zonneplan dynamisch contract with 2026 constants.
    pub fn zonneplan_2026() -> Self {
        Self {
            vat: VAT_NL,
            fee_incl: ZONNEPLAN_FEE_INCL,
            feed_in_fee: ZONNEPLAN_FEE_INCL,
            teruglever_kosten: ZONNEPLAN_TERUGLEVERKOSTEN,
            bonus: BonusRules::zonnebonus(),
            daylight: Some(Daylight::netherlands()),
            tax: EnergyTaxTable::zonneplan_2026(),
        }
    }

    /// Replace the yearly tax table (configuration, tests for later years).
    pub fn with_tax(mut self, tax: EnergyTaxTable) -> Self {
        self.tax = tax;
        self
    }

    /// The configured tax table.
    pub fn tax(&self) -> &EnergyTaxTable {
        &self.tax
    }

    /// Supplier fee excl. VAT: `fee_incl / (1 + vat)`.
    pub fn fee_ex(&self) -> f64 {
        self.fee_incl / (1.0 + self.vat)
    }

    /// Refuse constants that cannot produce a finite price.
    pub fn validate(&self) -> Result<(), PriceError> {
        if !self.vat.is_finite() || self.vat <= -1.0 {
            return Err(PriceError::Contract {
                field: "vat",
                value: self.vat,
            });
        }
        for (field, value) in [
            ("fee_incl", self.fee_incl),
            ("feed_in_fee", self.feed_in_fee),
            ("teruglever_kosten", self.teruglever_kosten),
        ] {
            if !value.is_finite() {
                return Err(PriceError::Contract { field, value });
            }
        }
        self.bonus.validate()?;
        EnergyTaxTable::new(self.tax.years().to_vec())?;
        Ok(())
    }

    /// Exact energy-tax entry for a tariff year.
    pub fn tax_year(&self, year: i32) -> Result<&TaxYear, PriceError> {
        self.tax.lookup(year).ok_or(PriceError::TaxYear { year })
    }

    /// `market = (buy_all_in - fee_incl) / (1 + vat) - energy_tax_ex` (prices.md §2.1).
    pub fn market_from_buy(&self, buy_all_in: f64, year: i32) -> Result<f64, PriceError> {
        if !buy_all_in.is_finite() {
            return Err(PriceError::Contract {
                field: "buy_all_in",
                value: buy_all_in,
            });
        }
        if !self.vat.is_finite() || self.vat <= -1.0 {
            return Err(PriceError::Contract {
                field: "vat",
                value: self.vat,
            });
        }
        let tax = self.tax_year(year)?;
        Ok((buy_all_in - self.fee_incl) / (1.0 + self.vat) - tax.energy_tax_ex)
    }

    /// The published build-up, inverted: `(market + energy_tax_ex) * (1 + vat) + fee_incl`.
    pub fn all_in(&self, market: f64, year: i32) -> Result<f64, PriceError> {
        let tax = self.tax_year(year)?;
        Ok((market + tax.energy_tax_ex) * (1.0 + self.vat) + self.fee_incl)
    }

    /// Whether the Zonnebonus applies to this slot/source at this year-to-date export.
    pub fn bonus_applies(&self, slot: &Slot, source: ExportSource, bonus_ytd_kwh: f64) -> bool {
        let raw = slot.market + self.feed_in_fee;
        let daylight = match self.daylight {
            Some(daylight) if self.bonus.daylight => daylight.is_daylight(slot.t_start),
            Some(_) => true,
            None => false,
        };
        self.bonus.pct > 0.0
            && raw > 0.0
            && (!self.bonus.pv_only || source.is_pv())
            && daylight
            && bonus_ytd_kwh < self.bonus.cap_kwh
    }

    /// Raw post-saldering feed-in price (may be negative; settlement clamps exports).
    pub fn feed_in_post(&self, slot: &Slot, source: ExportSource, bonus_ytd_kwh: f64) -> f64 {
        let multiplier = if self.bonus_applies(slot, source, bonus_ytd_kwh) {
            1.0 + self.bonus.pct
        } else {
            1.0
        };
        (slot.market + self.feed_in_fee) * multiplier - self.teruglever_kosten
    }
}
