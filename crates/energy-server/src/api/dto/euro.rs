//! Money in EUR, rounded to the ledger's 1e-6 resolution on the way out.

use serde::Serialize;
use serde::ser::Serializer;

/// An amount of EUR. Always finite, always rounded to six decimals.
///
/// The value is a JSON number (not a string) because the UI does arithmetic on it; the ledger's
/// resolution is preserved by rounding to 1e-6, which is exactly what it stores.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Euro(f64);

/// The ledger's money resolution (PLAN.md §2).
const MONEY_SCALE: f64 = 1_000_000.0;

impl Euro {
    /// Round `value` to EUR 1e-6. A non-finite value becomes zero (never `NaN` in JSON).
    pub(crate) fn new(value: f64) -> Self {
        if !value.is_finite() {
            return Self(0.0);
        }
        Self((value * MONEY_SCALE).round() / MONEY_SCALE)
    }
}

impl From<f64> for Euro {
    fn from(value: f64) -> Self {
        Self::new(value)
    }
}

impl Serialize for Euro {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0)
    }
}
