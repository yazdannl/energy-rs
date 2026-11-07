//! The price store: publication age, resolution, forward coverage, gaps and one worked derivation.

use chrono::{DateTime, Datelike, TimeZone, Utc};
use chrono_tz::Europe::Amsterdam;
use energy_price::{
    Contract, ExportSource, PriceError, PriceRepository, Publication, Settlement,
    SqlitePriceRepository,
};

use crate::bootstrap::options::Options;
use crate::diag::check::Check;
use crate::diag::status::Status;

/// Publication slot at which the next day's prices are expected (13:00 local).
const PUBLICATION_HOUR: u32 = 13;
/// How far ahead the doctor looks for coverage, seconds.
const LOOKAHEAD_S: i64 = 36 * 3600;
/// The planner refuses a book shorter than this, seconds.
const MIN_COVERAGE_S: i64 = 3_600;

/// Publication, coverage and derivation checks over the read-only price store.
pub fn checks(options: &Options, now: i64) -> Vec<Check> {
    let path = options.database();
    if !path.exists() {
        return vec![Check::warn(
            "prices store",
            format!(
                "no price store at {}; start the add-on once so prices are published",
                path.display()
            ),
        )];
    }
    let repository = match SqlitePriceRepository::open_read_only(&path) {
        Ok(repository) => repository,
        Err(error) => {
            return vec![Check::fail(
                "prices store",
                format!("{} cannot be read: {error}", path.display()),
            )];
        }
    };
    let contract = Contract::zonneplan_2026();
    let settlement = Settlement::new(contract.clone());
    vec![
        provider(options),
        publication(&repository, options, now),
        coverage(&repository, now),
        derivation(&repository, &contract, &settlement, now),
    ]
}

/// Which provider the options select, and whether it has what it needs to run.
fn provider(options: &Options) -> Check {
    let (choice, direct, sensor) = options.price_choice();
    let detail = format!("{choice} (direct token: {direct}, HA sensor: {sensor})");
    if choice == "zonneplan" && !direct {
        return Check::fail(
            "prices provider",
            format!("{detail}; price_provider is zonneplan but no refresh token is configured"),
        );
    }
    if direct {
        Check::pass("prices provider", detail)
    } else {
        Check::warn("prices provider", format!("{detail}; sensor fallback only"))
    }
}

fn publication(repository: &SqlitePriceRepository, options: &Options, now: i64) -> Check {
    match repository.latest_publication() {
        Err(error) => Check::fail("prices publication", format!("read failed: {error}")),
        Ok(None) => Check::fail("prices publication", "the store holds no publication"),
        Ok(Some(publication)) => publication_row(&publication, options, now),
    }
}

fn publication_row(publication: &Publication, options: &Options, now: i64) -> Check {
    let age = now.saturating_sub(publication.fetched_at).max(0);
    let resolution = publication.source.slot_seconds();
    let status = if resolution != 900 {
        Status::Fail
    } else if publication.last_t_end <= now || age > LOOKAHEAD_S {
        Status::Warn
    } else {
        Status::Pass
    };
    let (choice, _, _) = options.price_choice();
    let detail = format!(
        "{} slots {} .. {} ({}), {}s resolution, provider {choice}, fetched {} ago",
        publication.slot_count,
        local(publication.first_t_start),
        local(publication.last_t_end),
        publication.source,
        resolution,
        age_text(age),
    );
    checked("prices publication", status, detail)
}

fn coverage(repository: &SqlitePriceRepository, now: i64) -> Check {
    let end = now.saturating_add(LOOKAHEAD_S);
    match repository.load_book(now, end) {
        Ok(book) if book.is_empty() => Check::fail(
            "prices coverage",
            format!(
                "no slots for the next {} h; next publication expected {}",
                LOOKAHEAD_S / 3600,
                next_publication(now)
            ),
        ),
        Ok(book) => coverage_row(&book, now),
        Err(PriceError::Gap { t_start }) => Check::fail(
            "prices coverage",
            format!(
                "gap at {}; next publication expected {}",
                local(t_start),
                next_publication(now)
            ),
        ),
        Err(error) => Check::fail("prices coverage", error.to_string()),
    }
}

fn coverage_row(book: &energy_price::PriceBook, now: i64) -> Check {
    let covered_until = book.coverage_end().unwrap_or(now);
    let status = if covered_until < now.saturating_add(MIN_COVERAGE_S) {
        Status::Fail
    } else if covered_until < now.saturating_add(6 * 3600) {
        Status::Warn
    } else {
        Status::Pass
    };
    let detail = format!(
        "covers {} .. {} ({} slots, {:.1} h ahead); next publication expected {}",
        local(book.first().map_or(now, |slot| slot.t_start)),
        local(covered_until),
        book.len(),
        (covered_until - now) as f64 / 3600.0,
        next_publication(now)
    );
    checked("prices coverage", status, detail)
}

fn derivation(
    repository: &SqlitePriceRepository,
    contract: &Contract,
    settlement: &Settlement,
    now: i64,
) -> Check {
    let slot = match repository.load_book(now, now.saturating_add(MIN_COVERAGE_S)) {
        Ok(book) => book.first().copied(),
        Err(error) => return Check::fail("prices derivation", error.to_string()),
    };
    let Some(slot) = slot else {
        return Check::warn("prices derivation", "no slot covers the current moment");
    };
    let Some(year) = slot.local_date().map(|date| date.year()) else {
        return Check::fail("prices derivation", "slot has no local date");
    };
    let market = match contract.market_from_buy(slot.buy_all_in, year) {
        Ok(market) => market,
        Err(error) => {
            return Check::fail(
                "prices derivation",
                format!("market cannot be derived: {error}"),
            );
        }
    };
    let status = if (market - slot.market).abs() <= 1e-6 {
        Status::Pass
    } else {
        Status::Warn
    };
    let feed_in = settlement.feed_in_post(&slot, ExportSource::Battery);
    let export = settlement.export_price(&slot, ExportSource::Battery);
    let detail = format!(
        "{} buy {:.5}, market {:.5} (stored {:.5}), feed-in {:.5}, export {:.5} EUR/kWh",
        local(slot.t_start),
        slot.buy_all_in,
        market,
        slot.market,
        feed_in,
        export
    );
    checked("prices derivation", status, detail)
}

fn checked(name: &'static str, status: Status, detail: String) -> Check {
    match status {
        Status::Pass => Check::pass(name, detail),
        Status::Warn => Check::warn(name, detail),
        Status::Fail => Check::fail(name, detail),
    }
}

/// Local `Europe/Amsterdam` timestamp of an instant, seconds precision.
fn local(epoch: i64) -> String {
    DateTime::<Utc>::from_timestamp(epoch, 0).map_or_else(
        || epoch.to_string(),
        |utc| {
            utc.with_timezone(&Amsterdam)
                .format("%m-%d %H:%M")
                .to_string()
        },
    )
}

fn age_text(seconds: i64) -> String {
    if seconds < 3_600 {
        format!("{} min", seconds / 60)
    } else {
        format!("{:.1} h", seconds as f64 / 3600.0)
    }
}

/// The next local 13:00 at which tomorrow's prices are expected.
fn next_publication(now: i64) -> String {
    let Some(utc) = DateTime::<Utc>::from_timestamp(now, 0) else {
        return "-".to_owned();
    };
    let local = utc.with_timezone(&Amsterdam);
    let today = Amsterdam
        .with_ymd_and_hms(
            local.year(),
            local.month(),
            local.day(),
            PUBLICATION_HOUR,
            0,
            0,
        )
        .single();
    let target = match today {
        Some(today) if today.timestamp() > now => Some(today),
        _ => {
            let tomorrow = local.date_naive().succ_opt();
            tomorrow.and_then(|date| {
                Amsterdam
                    .with_ymd_and_hms(
                        date.year(),
                        date.month(),
                        date.day(),
                        PUBLICATION_HOUR,
                        0,
                        0,
                    )
                    .single()
            })
        }
    };
    target.map_or_else(
        || "-".to_owned(),
        |at| at.format("%m-%d %H:%M %Z").to_string(),
    )
}
