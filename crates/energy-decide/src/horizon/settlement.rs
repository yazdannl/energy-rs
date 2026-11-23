//! One signed export price rule for every slot, with a typed price reason.

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use chrono_tz::Europe::Amsterdam;

use crate::horizon::{BudgetState, HorizonRefusal, NetMeteringRegime, PriceSlot, SettlementReason};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotSettlement {
    /// Slot start, UTC epoch seconds.
    pub t_start: i64,
    /// Buy price, EUR/kWh.
    pub buy_eur_per_kwh: f64,
    /// Export and marginal export price, EUR/kWh, signed when feed-in is negative.
    pub export_eur_per_kwh: f64,
    /// Whether this slot falls inside the netting regime.
    pub netting: bool,
    /// Typed reason for the export price.
    pub reason: SettlementReason,
    /// Provenance of a 1:1 entitlement, otherwise the budget state.
    pub budget_source: &'static str,
}

/// Price one slot; the same signed export price is used at the margin.
pub fn settle_slot(
    slot: &PriceSlot,
    regime: &NetMeteringRegime,
    budget: &BudgetState,
    now: i64,
) -> Result<SlotSettlement, HorizonRefusal> {
    check_no_straddle(slot, regime)?;
    let netting = regime.is_netted(slot.t_start, now)?;
    let one_to_one = netting
        && (matches!(budget, BudgetState::UntrackedAcknowledged)
            || matches!(budget, BudgetState::Active { remaining_kwh } if *remaining_kwh > 0.0));
    let export = if one_to_one {
        slot.buy_all_in
    } else {
        slot.feed_in_post
    };
    Ok(SlotSettlement {
        t_start: slot.t_start,
        buy_eur_per_kwh: slot.buy_all_in,
        export_eur_per_kwh: export,
        netting,
        reason: price_reason(netting, budget),
        budget_source: budget_source(netting, one_to_one, budget),
    })
}

fn price_reason(netting: bool, budget: &BudgetState) -> SettlementReason {
    if !netting {
        return SettlementReason::NetMeteringInactive;
    }
    if matches!(budget, BudgetState::Active { remaining_kwh } if *remaining_kwh <= 0.0) {
        return SettlementReason::SalderingBudgetExhausted;
    }
    budget.reason()
}

fn budget_source(netting: bool, one_to_one: bool, budget: &BudgetState) -> &'static str {
    if one_to_one {
        budget.budget_source()
    } else if netting {
        "not_applicable"
    } else {
        "regime_inactive"
    }
}

/// 2027-01-01T00:00 Amsterdam, the exclusive end of the date-driven netting regime.
pub fn netting_boundary_utc() -> i64 {
    NaiveDate::from_ymd_opt(2027, 1, 1)
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .and_then(|local| Amsterdam.from_local_datetime(&local).earliest())
        .map_or(i64::MIN, |instant| instant.timestamp())
}

fn check_no_straddle(slot: &PriceSlot, regime: &NetMeteringRegime) -> Result<(), HorizonRefusal> {
    if !matches!(regime, NetMeteringRegime::DateDriven) {
        return Ok(());
    }
    let boundary = netting_boundary_utc();
    let end = slot.t_start.saturating_add(slot.slot_seconds);
    if slot.t_start < boundary && end > boundary {
        return Err(HorizonRefusal::RegimeBoundaryCrossing {
            t_start: slot.t_start,
            t_end: end,
            boundary,
        });
    }
    Ok(())
}

/// Kept for callers that need the local year of a slot explicitly.
pub fn local_year(t_start: i64) -> Option<i32> {
    DateTime::<Utc>::from_timestamp(t_start, 0)
        .map(|instant| instant.with_timezone(&Amsterdam).year())
}
