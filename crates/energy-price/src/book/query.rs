//! A fetch request: the local date range to fill, the instant used for staleness checks, and the
//! coverage the caller needs.  Publication timing is explicit here instead of baked into providers
//! (before ~13:00 CET tomorrow's slots do not exist yet, and that is not an error).

use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeZone};
use chrono_tz::Tz;

use crate::error::PriceError;

/// `Europe/Amsterdam`, the tariff timezone for dates, years and publication timing.
pub const TARIFF_TZ: Tz = chrono_tz::Europe::Amsterdam;

/// A local date range keyed by UTC instants, plus `now` and the required horizon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceQuery {
    from: NaiveDate,
    to: NaiveDate,
    now: i64,
    horizon_secs: i64,
    start_utc: i64,
    end_utc: i64,
}

impl PriceQuery {
    /// Build a query for the inclusive local dates `from ..= to`, requiring coverage through
    /// `now + horizon_secs`.  An empty or inverted range is refused.
    pub fn new(
        from: NaiveDate,
        to: NaiveDate,
        now: i64,
        horizon_secs: i64,
    ) -> Result<Self, PriceError> {
        if from > to {
            return Err(PriceError::Payload(format!(
                "price range {from} ..= {to} is inverted"
            )));
        }
        if horizon_secs < 0 {
            return Err(PriceError::Payload(format!(
                "price horizon {horizon_secs} is negative"
            )));
        }
        let start_utc = local_midnight(from).ok_or_else(|| {
            PriceError::Payload(format!("local midnight of {from} is not representable"))
        })?;
        let end_utc = next_day(to).and_then(local_midnight).ok_or_else(|| {
            PriceError::Payload(format!("local midnight after {to} is not valid"))
        })?;
        Ok(Self {
            from,
            to,
            now,
            horizon_secs,
            start_utc,
            end_utc,
        })
    }

    /// First local date in the range.
    pub const fn from(&self) -> NaiveDate {
        self.from
    }

    /// Last local date in the range.
    pub const fn to(&self) -> NaiveDate {
        self.to
    }

    /// Instant used for past-slot filtering and staleness.
    pub const fn now(&self) -> i64 {
        self.now
    }

    /// Coverage the caller wants after `now`, in seconds.
    ///
    /// Providers deliver whatever is published (a slot covering `now` and nothing more, before the
    /// ~13:00 CET publication) and never extend the book; the caller compares this against
    /// [`PriceBook::coverage_end`](crate::book::PriceBook::coverage_end) and shortens its horizon.
    pub const fn horizon_secs(&self) -> i64 {
        self.horizon_secs
    }

    /// Inclusive UTC start instant of the first local day.
    pub const fn start_utc(&self) -> i64 {
        self.start_utc
    }

    /// Exclusive UTC end instant after the last local day.
    pub const fn end_utc(&self) -> i64 {
        self.end_utc
    }

    /// Whether a slot start instant belongs to the requested local dates.
    pub const fn contains(&self, t_start: i64) -> bool {
        t_start >= self.start_utc && t_start < self.end_utc
    }

    /// Number of local days covered by the request.
    pub fn days(&self) -> i64 {
        self.to
            .signed_duration_since(self.from)
            .num_days()
            .saturating_add(1)
    }
}

/// UTC instant of Amsterdam local midnight for `date`.
pub fn local_midnight(date: NaiveDate) -> Option<i64> {
    let naive = date.and_hms_opt(0, 0, 0)?;
    TARIFF_TZ
        .from_local_datetime(&naive)
        .earliest()
        .map(|local| local.timestamp())
}

fn next_day(date: NaiveDate) -> Option<NaiveDate> {
    date.succ_opt()
}

/// Local date of an instant in the tariff timezone.
pub fn local_date_of(t_start: i64) -> Option<NaiveDate> {
    chrono::DateTime::from_timestamp(t_start, 0)
        .map(|utc| utc.with_timezone(&TARIFF_TZ).date_naive())
}

/// Local year of an instant in the tariff timezone.
pub fn local_year_of(t_start: i64) -> Option<i32> {
    chrono::DateTime::from_timestamp(t_start, 0).map(|utc| utc.with_timezone(&TARIFF_TZ).year())
}

/// Whether a naive local timestamp is unambiguous enough to settle (earliest on DST overlap).
pub fn local_naive_to_utc(naive: NaiveDateTime) -> Option<i64> {
    TARIFF_TZ
        .from_local_datetime(&naive)
        .earliest()
        .map(|local| local.timestamp())
}
