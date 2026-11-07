//! Measured 15-minute history and the EWMA fallback for a missing forecast.
//!
//! The history is the plan's sanity anchor: it is where a fallback forecast comes from and it is
//! also what proves the energy dashboard statistics are usable. It is therefore validated strictly
//! -- contiguous quarter-hours, finite values, no holes -- and a hole is a typed refusal
//! ([`PlanError::StatsGap`]). A zero-fill here would look like a house with no load and a PV array
//! in the dark, which the optimizer would happily charge from the grid for.

use chrono::{DateTime, Timelike, Utc};
use energy_price::book::query::TARIFF_TZ;

use crate::plan::{PlanError, SLOT_SECONDS};
use crate::plan_source::Forecast;

/// EWMA weight of the previous day's sample for the same slot of the day.
const EWMA_ALPHA: f64 = 0.35;
/// Shortest usable history: one full day, so every slot of the day has a sample.
const MIN_HISTORY_SLOTS: usize = 96;

/// Validated per-slot measurements behind a plan.
///
/// One entry per contiguous quarter-hour: PV production and derived house load, both in kW. The
/// values come from the energy dashboard's `change` statistics, so they are energy per slot
/// converted to average power, never an instantaneous sensor read.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredHistory {
    starts: Vec<i64>,
    pv_kw: Vec<f64>,
    load_kw: Vec<f64>,
}

impl MeasuredHistory {
    /// Validate and wrap one contiguous series of quarter-hour measurements.
    ///
    /// Every start must be on a quarter-hour boundary and exactly 15 minutes after the previous
    /// one; every value must be finite. Anything else is a gap, and a gap is refused.
    pub fn new(starts: Vec<i64>, pv_kw: Vec<f64>, load_kw: Vec<f64>) -> Result<Self, PlanError> {
        if starts.is_empty() || starts.len() != pv_kw.len() || starts.len() != load_kw.len() {
            return Err(gap("pv/load", starts.first().copied().unwrap_or(0)));
        }
        for (index, start) in starts.iter().enumerate() {
            if start.rem_euclid(SLOT_SECONDS) != 0 {
                return Err(gap("pv/load", *start));
            }
            if index > 0 && *start != starts[index - 1].saturating_add(SLOT_SECONDS) {
                return Err(gap("pv/load", *start));
            }
            if !pv_kw[index].is_finite() || !load_kw[index].is_finite() {
                return Err(gap("pv/load", *start));
            }
        }
        Ok(Self {
            starts,
            pv_kw,
            load_kw,
        })
    }

    /// Number of measured slots.
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    /// Whether nothing was measured.
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// First measured slot start, epoch seconds.
    pub fn first(&self) -> Option<i64> {
        self.starts.first().copied()
    }

    /// Exclusive end of the measured coverage, epoch seconds.
    pub fn coverage_end(&self) -> Option<i64> {
        self.starts
            .last()
            .map(|start| start.saturating_add(SLOT_SECONDS))
    }

    /// Total measured load over the last `hours`, kWh; the load fallback's base level.
    pub fn load_kwh_over(&self, hours: i64) -> f64 {
        let wanted = hours.max(0).saturating_mul(3600) / SLOT_SECONDS;
        let slots = usize::try_from(wanted).unwrap_or(0);
        let from = self.starts.len().saturating_sub(slots);
        let quarter_hours = SLOT_SECONDS as f64 / 3600.0;
        self.load_kw
            .get(from..)
            .map_or(0.0, |values| values.iter().sum::<f64>() * quarter_hours)
    }

    /// The EWMA fallback for `slots` quarter-hours from `from`, or `None` without a full day.
    ///
    /// Each target slot takes the same clock slot (local `Europe/Amsterdam`) of the previous days,
    /// newest first, weighted `(1 - alpha)^rank`: the last three days dominate and older days only
    /// steady the edge. A slot of the day with no sample at all makes the whole fallback
    /// unavailable, which the caller turns into a typed refusal -- a partially fabricated series is
    /// exactly the zero-fill this module exists to prevent.
    pub fn fallback(&self, from: i64, slots: usize, load: bool) -> Option<Vec<f64>> {
        if self.starts.len() < MIN_HISTORY_SLOTS {
            return None;
        }
        let values = if load { &self.load_kw } else { &self.pv_kw };
        let mut series = Vec::with_capacity(slots);
        for index in 0..slots {
            let offset = i64::try_from(index).ok()?.checked_mul(SLOT_SECONDS)?;
            let target = from.checked_add(offset)?;
            let tod = slot_of_day(target)?;
            let (sum, weight) = self.weighted_same_slot(tod, values)?;
            series.push(sum / weight);
        }
        Some(series)
    }

    /// Recency-weighted sum of every measured sample at one clock slot.
    fn weighted_same_slot(&self, tod: u32, values: &[f64]) -> Option<(f64, f64)> {
        let mut sum = 0.0;
        let mut weight = 0.0;
        let mut rank = 0_u32;
        for (position, start) in self.starts.iter().enumerate().rev() {
            if slot_of_day(*start)? != tod {
                continue;
            }
            let factor = (1.0 - EWMA_ALPHA).powi(i32::try_from(rank).ok()?);
            sum += values.get(position)?.abs() * factor;
            weight += factor;
            rank = rank.saturating_add(1);
        }
        (weight > 0.0).then_some((sum, weight))
    }

    /// The degraded forecast built from this history, or the reason it cannot be built.
    pub fn degraded_forecast(
        &self,
        from: i64,
        slots: usize,
        as_of: i64,
        note: impl Into<String>,
    ) -> Result<Forecast, PlanError> {
        let pv = self.fallback(from, slots, false).ok_or_else(|| {
            unavailable("pv_kw", "measured history has no full day for the fallback")
        })?;
        let load = self.fallback(from, slots, true).ok_or_else(|| {
            unavailable(
                "load_kw",
                "measured history has no full day for the fallback",
            )
        })?;
        Ok(Forecast::degraded(pv, load, as_of, note))
    }
}

/// The clock slot of the day (0..96) in the tariff timezone.
fn slot_of_day(t: i64) -> Option<u32> {
    let local = DateTime::<Utc>::from_timestamp(t, 0)?.with_timezone(&TARIFF_TZ);
    Some(local.hour() * 4 + local.minute() / 15)
}

fn gap(series: &'static str, t_start: i64) -> PlanError {
    PlanError::StatsGap { series, t_start }
}

fn unavailable(name: &'static str, reason: &str) -> PlanError {
    PlanError::ForecastUnavailable {
        name,
        reason: reason.to_owned(),
    }
}
