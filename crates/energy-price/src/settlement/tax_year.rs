//! Energiebelasting per year, excl. VAT, in EUR/kWh.  Rates are versioned by year instead of being
//! hard-coded in formulas: 2026 is known, later years must come from configuration.

use crate::error::PriceError;

/// One year of the per-kWh energy tax (excl. VAT).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaxYear {
    /// Tariff year (calendar year in `Europe/Amsterdam`).
    pub year: i32,
    /// Energiebelasting, EUR/kWh, excluding VAT.
    pub energy_tax_ex: f64,
}

impl TaxYear {
    /// 2026 schijf 1 rate, from EnergieTeller / Belastingdienst (prices.md §7 row 1).
    pub const ENERGY_TAX_2026: f64 = 0.09161;

    /// A rate entry; validity is checked by [`EnergyTaxTable::new`].
    pub const fn new(year: i32, energy_tax_ex: f64) -> Self {
        Self { year, energy_tax_ex }
    }

    /// Refuse a year or a rate that cannot be settled.
    pub fn validate(&self) -> Result<(), PriceError> {
        if !(1970..=2200).contains(&self.year) {
            return Err(PriceError::Payload(format!(
                "energy tax year {} is outside 1970..=2200",
                self.year
            )));
        }
        if !self.energy_tax_ex.is_finite() || self.energy_tax_ex < 0.0 {
            return Err(PriceError::Contract {
                field: "energy_tax_ex",
                value: self.energy_tax_ex,
            });
        }
        Ok(())
    }
}
