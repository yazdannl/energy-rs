//! The one precedence rule for a pack's AC round trip: **learned > manual > typed default**.
//!
//! Every place that derives an efficiency — the dispatch catalog, the plan probe's [`PackSpec`]
//! inputs and the packs API — calls [`resolve`] with the same two candidates and uses its value
//! and source. Candidates are *not* the raw stored numbers:
//!
//! * `learned` is a published measurement only: a learning profile whose `efficiency_applied` is
//!   set, or a ledger row stamped after the never-measured sentinel. A default profile that has
//!   never seen a closed cycle must not outrank the operator's entry. A learned value still wins
//!   once it is confident — the manual number is a **pre-learning** default, not an override.
//! * `manual` is the operator's per-pack entry in the `packs.manual_round_trip` column, accepted
//!   only when finite and inside [`MIN_ROUND_TRIP`]..[`MAX_ROUND_TRIP`]. A corrupt stored value is
//!   ignored here, never repaired or promoted into a learned profile.
//!
//! When neither candidate survives, [`DEFAULT_ROUND_TRIP`] applies. Both legs split it evenly
//! (`sqrt`), the convention the UI and the configuration use.

use super::efficiency::{DEFAULT_ROUND_TRIP, MAX_ROUND_TRIP, MIN_ROUND_TRIP};
use super::profile::NEVER_MEASURED_AT;

/// Where the effective round trip came from; serialized as `learned`, `manual` or `default`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundTripSource {
    /// A measured, confident learning result.
    Learned,
    /// The operator's per-pack entry, used before learning published a result.
    Manual,
    /// The typed fallback constant.
    Default,
}

impl RoundTripSource {
    /// The stable name used by the API and the logs.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Learned => "learned",
            Self::Manual => "manual",
            Self::Default => "default",
        }
    }
}

/// A ledger row's round trip counts as learned only once a pass published a measurement.
///
/// Unmeasured passes store the typed seed with the never-measured stamp; treating that as a
/// measurement would hide the operator's entry forever.
pub fn published_round_trip(value: Option<f64>, learned_at: Option<i64>) -> Option<f64> {
    let published = learned_at.is_some_and(|at| at > NEVER_MEASURED_AT.timestamp());
    published.then_some(value).flatten()
}

/// Whether a stored manual fraction is usable. Finite first: `NaN` compares false everywhere, so
/// it must never reach a range check unchecked.
pub fn valid_manual(value: f64) -> bool {
    value.is_finite() && (MIN_ROUND_TRIP..=MAX_ROUND_TRIP).contains(&value)
}

/// Whether an operator-entered percentage maps to a usable fraction.
pub fn valid_percent(percent: f64) -> bool {
    percent.is_finite() && (MIN_ROUND_TRIP * 100.0..=MAX_ROUND_TRIP * 100.0).contains(&percent)
}

/// Convert a validated percentage to the stored fraction.
pub fn from_percent(percent: f64) -> f64 {
    percent / 100.0
}

/// Convert a fraction to the percentage shown in the API.
pub fn to_percent(value: f64) -> f64 {
    value * 100.0
}

/// Resolve the effective AC round trip and its provenance.
pub fn resolve(learned: Option<f64>, manual: Option<f64>) -> (f64, RoundTripSource) {
    if let Some(value) = learned.filter(|value| value.is_finite() && *value > 0.0 && *value <= 1.0)
    {
        return (value, RoundTripSource::Learned);
    }
    if let Some(value) = manual.filter(|value| valid_manual(*value)) {
        return (value, RoundTripSource::Manual);
    }
    (DEFAULT_ROUND_TRIP, RoundTripSource::Default)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]
mod tests {
    use super::*;

    #[test]
    fn manual_round_trip_wins_over_the_typed_default() {
        assert_eq!(resolve(None, Some(0.83)), (0.83, RoundTripSource::Manual));
    }

    #[test]
    fn a_confident_learned_round_trip_overrides_the_manual_entry() {
        assert_eq!(
            resolve(Some(0.92), Some(0.83)),
            (0.92, RoundTripSource::Learned)
        );
    }

    #[test]
    fn invalid_candidates_fall_back_in_the_documented_order() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -0.5, 1.2] {
            let (value, source) = resolve(Some(bad), Some(0.83));
            assert_eq!((value, source), (0.83, RoundTripSource::Manual), "{bad}");
            let (value, source) = resolve(Some(0.92), Some(bad));
            assert_eq!((value, source), (0.92, RoundTripSource::Learned), "{bad}");
        }
    }

    #[test]
    fn an_unpublished_row_has_no_learned_round_trip() {
        assert_eq!(published_round_trip(Some(0.81), None), None);
        assert_eq!(
            published_round_trip(Some(0.81), Some(NEVER_MEASURED_AT.timestamp())),
            None
        );
        assert_eq!(
            published_round_trip(Some(0.92), Some(NEVER_MEASURED_AT.timestamp() + 1)),
            Some(0.92)
        );
    }

    #[test]
    fn percent_values_round_trip_exactly() {
        for value in [MIN_ROUND_TRIP, 0.81, 0.83, MAX_ROUND_TRIP] {
            let percent = to_percent(value);
            assert!(valid_percent(percent));
            assert_eq!(from_percent(percent), value);
        }
        assert_eq!(to_percent(0.81), 81.0);
    }

    #[test]
    fn percentages_outside_the_physical_envelope_are_invalid() {
        for percent in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            to_percent(MIN_ROUND_TRIP) - 0.1,
            to_percent(MAX_ROUND_TRIP) + 0.1,
        ] {
            assert!(!valid_percent(percent), "{percent}");
        }
        assert!(valid_percent(81.0));
        assert!(valid_percent(50.0));
        assert!(valid_percent(100.0));
    }
}
