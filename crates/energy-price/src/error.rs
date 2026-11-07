//! Typed errors: one variant per reason a price, a book or a publication was refused.

use chrono::NaiveDate;
use thiserror::Error;

/// Everything this crate refuses.  Price handling is exact-or-refuse: a missing or malformed slot
/// never becomes a number, and a failed refresh keeps the previous publication instead of guessing.
#[derive(Debug, Error)]
pub enum PriceError {
    #[error("price request to {url} failed: {reason}")]
    Http { url: String, reason: String },
    #[error("price authorisation failed: {0}")]
    Auth(String),
    #[error("price payload is not usable: {0}")]
    Payload(String),
    #[error("price payload is the gas tariff, not electricity")]
    GasDecoy,
    #[error("no electricity prices published for {from} ..= {to}")]
    NotPublished { from: NaiveDate, to: NaiveDate },
    #[error("price grid has a gap at {t_start}")]
    Gap { t_start: i64 },
    #[error("price grid is not on a {expected}-second boundary at {t_start}")]
    Boundary { t_start: i64, expected: i64 },
    #[error("price slot {t_start} appears twice")]
    Duplicate { t_start: i64 },
    #[error("price coverage ends at {covered_until}, need {needed_until}")]
    Stale {
        covered_until: i64,
        needed_until: i64,
    },
    #[error("price {t_start} is not finite")]
    NonFinite { t_start: i64 },
    #[error("no energy tax configured for {year}")]
    TaxYear { year: i32 },
    #[error("contract constant {field} is unusable: {value}")]
    Contract { field: &'static str, value: f64 },
    #[error("price store failed: {0}")]
    Store(String),
}
