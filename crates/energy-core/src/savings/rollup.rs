//! Writing measured savings: closed days, today's provisional value, and catch-up.

use std::sync::Arc;

use chrono::NaiveDate;
use energy_ledger::Ledger;
use energy_price::Settlement;
use energy_price::book::query::local_date_of;

use super::{RollupReport, SavingsError, SavingsSource, account};

/// Days the rollup goes back when it has no resume point, and the most it will recompute in one
/// pass after the add-on was down for a while.
pub const CATCH_UP_DAYS: i64 = 31;

/// The daily savings writer.
///
/// One pass recomputes every closed day that is missing plus the current day, in day order, from
/// measured data only. Writing is an upsert, so a repeated pass over the same measurements stores
/// byte-identical rows and a day that lost coverage has its numbers replaced by `NULL`.
pub struct SavingsRollup {
    ledger: Arc<Ledger>,
    source: Arc<dyn SavingsSource>,
    settlement: Settlement,
}

impl SavingsRollup {
    /// A rollup over the ledger and one measured source.
    pub fn new(
        ledger: Arc<Ledger>,
        source: Arc<dyn SavingsSource>,
        settlement: Settlement,
    ) -> Self {
        Self {
            ledger,
            source,
            settlement,
        }
    }

    /// Recompute the missing closed days and refresh today's provisional row.
    ///
    /// A day that cannot be read (Home Assistant down, a missing statistic) is journalled and
    /// skipped: one bad day never stops the rollup, and it is retried on the next pass.
    pub async fn refresh(&self, now: i64) -> Result<RollupReport, SavingsError> {
        let today = local_date_of(now).ok_or(SavingsError::Clock)?;
        let mut report = RollupReport::default();
        for day in self.days(today)? {
            self.write_day(day, today, &mut report).await?;
        }
        log(&report);
        Ok(report)
    }

    /// Recompute (or skip and journal) one day, then store its row.
    async fn write_day(
        &self,
        day: NaiveDate,
        today: NaiveDate,
        report: &mut RollupReport,
    ) -> Result<(), SavingsError> {
        let reading = match self.source.read_day(day).await {
            Ok(reading) => reading,
            Err(error) => {
                tracing::warn!(%day, %error, "savings day skipped");
                report.failures.push(format!("{day}: {error}"));
                return Ok(());
            }
        };
        let closed = day < today;
        let row = account(&reading, &self.settlement).row(closed);
        self.ledger.savings()?.upsert(&row)?;
        report.record(&row.day_local, row.complete, closed);
        Ok(())
    }

    /// The days one pass must write: from the resume point through today.
    ///
    /// The resume point is the day after the newest stored closed day; an incomplete newest day is
    /// retried (late statistics can still fill it), and a ledger with no rows at all looks back
    /// [`CATCH_UP_DAYS`] days. Never more than [`CATCH_UP_DAYS`] days are recomputed in one pass.
    fn days(&self, today: NaiveDate) -> Result<Vec<NaiveDate>, SavingsError> {
        let floor = today - chrono::Duration::days(CATCH_UP_DAYS);
        let newest = self.ledger.savings()?.newest_closed(&today.to_string())?;
        let resume = match newest {
            Some(day) if day.complete => NaiveDate::parse_from_str(&day.day_local, "%Y-%m-%d")
                .ok()
                .and_then(|day| day.succ_opt())
                .unwrap_or(today),
            Some(day) => NaiveDate::parse_from_str(&day.day_local, "%Y-%m-%d").unwrap_or(today),
            None => floor,
        };
        let mut day = resume.clamp(floor, today);
        let mut days = Vec::new();
        while day <= today {
            days.push(day);
            let Some(next) = day.succ_opt() else {
                break;
            };
            day = next;
        }
        Ok(days)
    }
}

/// One line per pass at `debug`; a day that could not be read is warned where it happens.
fn log(report: &RollupReport) {
    tracing::debug!(
        days = report.days.len(),
        complete = report.complete,
        incomplete = report.incomplete,
        provisional = report.provisional,
        failures = report.failures.len(),
        "savings rollup pass"
    );
}
