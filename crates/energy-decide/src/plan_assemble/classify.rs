//! Required versus optional obligations from deadline and pack eligibility.

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Europe::Amsterdam;

use crate::horizon::Horizon;
use crate::plan_assemble::{Obligation, ObligationClass};

/// Required only when the deadline is inside the horizon and the pack is eligible.
pub fn obligation_class(obligation: &Obligation, horizon: &Horizon) -> ObligationClass {
    if !obligation.eligible
        || deadline_instant(&obligation.deadline, horizon.t0, horizon.coverage_end()).is_none()
    {
        return ObligationClass::Optional;
    }
    ObligationClass::Required
}

pub fn deadline_instant(deadline: &str, t0: i64, end: i64) -> Option<i64> {
    let (hour, minute) = parse_hhmm(deadline)?;
    let tz = Amsterdam;
    let first = DateTime::<Utc>::from_timestamp(t0, 0)?
        .with_timezone(&tz)
        .date_naive();
    let last = DateTime::<Utc>::from_timestamp(end, 0)?
        .with_timezone(&tz)
        .date_naive();
    let mut date = first;
    loop {
        let naive = date.and_hms_opt(hour, minute, 0)?;
        if let Some(local) = tz.from_local_datetime(&naive).earliest() {
            let stamp = local.timestamp();
            if stamp >= t0 && stamp < end {
                return Some(stamp);
            }
        }
        if date >= last {
            return None;
        }
        date = date.succ_opt()?;
    }
}

fn parse_hhmm(deadline: &str) -> Option<(u32, u32)> {
    let time = deadline.strip_suffix(" local").unwrap_or(deadline);
    let (hour, minute) = time.split_once(':')?;
    Some((hour.parse().ok()?, minute.parse().ok()?))
}
