//! Yearly energy-tax table.  Lookup is exact: an unknown year is refused rather than settled with
//! last year's rate (the 2027 rate was not definitive when prices.md was written).

use crate::error::PriceError;
use crate::settlement::TaxYear;

/// Energy tax rates by tariff year.
#[derive(Debug, Clone, PartialEq)]
pub struct EnergyTaxTable {
    years: Vec<TaxYear>,
}

const FALLBACK: TaxYear = TaxYear::new(2026, TaxYear::ENERGY_TAX_2026);

impl EnergyTaxTable {
    /// Build a table: at least one year, unique years, finite non-negative rates.
    pub fn new(mut years: Vec<TaxYear>) -> Result<Self, PriceError> {
        if years.is_empty() {
            return Err(PriceError::Payload(
                "energy tax table has no years".to_owned(),
            ));
        }
        years.sort_by_key(|entry| entry.year);
        for entry in &years {
            entry.validate()?;
        }
        for pair in years.windows(2) {
            if let [first, second] = pair
                && first.year == second.year
            {
                return Err(PriceError::Payload(format!(
                    "energy tax year {} appears twice",
                    first.year
                )));
            }
        }
        Ok(Self { years })
    }

    /// A table with only the 2026 rate.
    pub fn zonneplan_2026() -> Self {
        Self {
            years: vec![TaxYear::new(2026, TaxYear::ENERGY_TAX_2026)],
        }
    }

    /// Exact year lookup; `None` means the contract has not been configured for that year.
    pub fn lookup(&self, year: i32) -> Option<&TaxYear> {
        self.years.iter().find(|entry| entry.year == year)
    }

    /// All years, ascending.
    pub fn years(&self) -> &[TaxYear] {
        &self.years
    }

    /// The newest configured year (used by reporting, never by settlement).
    pub fn newest(&self) -> &TaxYear {
        self.years.last().unwrap_or(&FALLBACK)
    }
}

impl Default for EnergyTaxTable {
    fn default() -> Self {
        Self::zonneplan_2026()
    }
}
