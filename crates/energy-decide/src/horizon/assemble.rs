//! M1 precedence: request, resolution, interior gap, coverage, stale, inputs, forecast.

use crate::horizon::{
    Coverage, Horizon, HorizonAttempt, HorizonInput, HorizonOutcome, HorizonRefusal,
    NetMeteringRegime, NoticeLog, PriceSlot, build_schedule, sum_grid_sources,
};
use crate::{MIN_HORIZON_SLOTS, SLOT_SECONDS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Scan {
    actual: usize,
    first_missing: Option<i64>,
    missing_count: usize,
    tail: bool,
}

/// Assemble an attempt; the result is either a horizon or a typed refusal with full evidence.
pub fn assemble_horizon(input: &HorizonInput) -> HorizonAttempt {
    let mut attempt = HorizonAttempt::new(input.requested_slots, MIN_HORIZON_SLOTS);
    match build(input, &mut attempt) {
        Ok(horizon) => attempt.result = Ok(horizon),
        Err(refusal) => attempt.result = Err(refusal),
    }
    attempt
}

fn build(input: &HorizonInput, attempt: &mut HorizonAttempt) -> Result<Horizon, HorizonRefusal> {
    validate_request(input.requested_slots)?;
    let t0 = slot_start(input.now);
    let priced = required_slots(&input.slots, t0, input.requested_slots)?;
    let scan = scan_coverage(&priced, t0, input.requested_slots);
    record_coverage(attempt, &scan, t0);
    if let Some(first) = scan.first_missing
        && !scan.tail
    {
        return Err(HorizonRefusal::PriceGap {
            first_missing_t_start: first,
            missing_count: scan.missing_count,
        });
    }
    if scan.actual < MIN_HORIZON_SLOTS {
        return Err(HorizonRefusal::InsufficientCoverage {
            actual: scan.actual,
            requested: input.requested_slots,
            minimum: MIN_HORIZON_SLOTS,
        });
    }
    check_staleness(input.now, input.published_at, input.price_max_age_s)?;
    let mut notices = NoticeLog::default();
    let summed = sum_grid_sources(
        &input.grid_sources,
        scan.actual,
        &mut notices,
        &mut attempt.grid_source_coverage,
    );
    // Diagnostic evidence must survive both the success and the refusal path.
    attempt.notices = notices.emitted;
    let series = summed?;
    if !input.forecast_available {
        return Err(HorizonRefusal::ForecastUnavailable {
            reason: "no_fresh_forecast",
        });
    }
    let regime = NetMeteringRegime::from_config(
        &input.net_metering,
        input.now,
        input.operator_override.clone(),
    )?;
    let covered: Vec<PriceSlot> = priced.into_iter().take(scan.actual).collect();
    let schedule = build_schedule(&covered, &regime, &input.budget, input.now)?;
    let outcome = if scan.actual == input.requested_slots {
        HorizonOutcome::Healthy
    } else {
        HorizonOutcome::DegradedAccepted
    };
    Ok(Horizon {
        t0,
        requested_slots: input.requested_slots,
        coverage: attempt.coverage,
        slots: covered,
        net_metering: schedule,
        grid_series: series.values,
        grid_source_coverage: series.coverage,
        outcome,
    })
}

fn validate_request(requested: usize) -> Result<(), HorizonRefusal> {
    if requested == 0 || requested > crate::REQUESTED_SLOTS {
        return Err(HorizonRefusal::HorizonSlots { slots: requested });
    }
    Ok(())
}

fn slot_offset(index: usize) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX)
}

fn slot_start(now: i64) -> i64 {
    now.div_euclid(SLOT_SECONDS).saturating_mul(SLOT_SECONDS)
}

fn required_slots(
    all: &[PriceSlot],
    t0: i64,
    requested: usize,
) -> Result<Vec<PriceSlot>, HorizonRefusal> {
    let end = t0.saturating_add(slot_offset(requested).saturating_mul(SLOT_SECONDS));
    for slot in all {
        if slot.t_start >= t0 && slot.t_start < end && slot.slot_seconds != SLOT_SECONDS {
            return Err(HorizonRefusal::PriceResolution {
                expected_s: SLOT_SECONDS,
                got_s: slot.slot_seconds,
                t_start: slot.t_start,
            });
        }
    }
    let mut slots: Vec<PriceSlot> = all
        .iter()
        .copied()
        .filter(|slot| {
            slot.slot_seconds == SLOT_SECONDS
                && slot.t_start >= t0
                && slot.t_start < end
                && (slot.t_start - t0) % SLOT_SECONDS == 0
        })
        .collect();
    slots.sort_by_key(|slot| slot.t_start);
    slots.dedup_by_key(|slot| slot.t_start);
    Ok(slots)
}

fn scan_coverage(priced: &[PriceSlot], t0: i64, requested: usize) -> Scan {
    let present = |index: usize| {
        let time = t0.saturating_add(slot_offset(index).saturating_mul(SLOT_SECONDS));
        priced
            .binary_search_by_key(&time, |slot| slot.t_start)
            .is_ok()
    };
    let Some(first_index) = (0..requested).find(|index| !present(*index)) else {
        return Scan {
            actual: requested,
            first_missing: None,
            missing_count: 0,
            tail: false,
        };
    };
    let count = (first_index..requested)
        .take_while(|index| !present(*index))
        .count();
    Scan {
        actual: first_index,
        first_missing: Some(
            t0.saturating_add(slot_offset(first_index).saturating_mul(SLOT_SECONDS)),
        ),
        missing_count: count,
        tail: first_index + count == requested,
    }
}

fn record_coverage(attempt: &mut HorizonAttempt, scan: &Scan, t0: i64) {
    attempt.coverage = Coverage {
        requested_slots: attempt.requested_slots,
        actual_slots: scan.actual,
        minimum_slots: MIN_HORIZON_SLOTS,
        coverage_end: (scan.actual > 0)
            .then(|| t0.saturating_add(slot_offset(scan.actual).saturating_mul(SLOT_SECONDS))),
        missing_slots: if scan.tail { scan.missing_count } else { 0 },
        first_missing_t_start: scan.first_missing,
    };
    attempt.provisional_outcome = if scan.tail {
        HorizonOutcome::TailAccepted
    } else {
        HorizonOutcome::Healthy
    };
}

fn check_staleness(now: i64, published_at: i64, limit_s: i64) -> Result<(), HorizonRefusal> {
    let age = now.saturating_sub(published_at);
    if age > limit_s {
        return Err(HorizonRefusal::StalePrices {
            age_s: age,
            limit_s,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_start_floors_now_to_the_containing_quarter_hour() {
        let now = 15 * 3600 + 47 * 60;
        assert_eq!(slot_start(now), 15 * 3600 + 45 * 60);
    }

    #[test]
    fn scan_distinguishes_tail_from_interior_gap() {
        let start = 1_000_000;
        let mut slots: Vec<PriceSlot> = (0..108)
            .map(|index| PriceSlot {
                t_start: start + index * SLOT_SECONDS,
                slot_seconds: SLOT_SECONDS,
                buy_all_in: 0.4,
                feed_in_post: 0.18,
            })
            .collect();
        let tail = scan_coverage(&slots, start, 120);
        assert!(tail.tail);
        assert_eq!(tail.actual, 108);
        slots.remove(40);
        let gap = scan_coverage(&slots, start, 120);
        assert!(!gap.tail);
        assert_eq!(gap.missing_count, 1);
    }
}
