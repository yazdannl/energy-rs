//! A validated, contiguous price book.  Gaps are refused, never zero-filled; a book may start or
//! end early (tomorrow not published yet) and callers shorten their horizon accordingly.

use chrono::{DateTime, Utc};

use crate::book::Slot;
use crate::error::PriceError;

/// A gap-free run of slots from one publication.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PriceBook {
    slots: Vec<Slot>,
}

impl PriceBook {
    /// Sort by start instant, then require finite prices, one resolution, boundary-aligned and
    /// strictly contiguous slots.  Duplicates and holes are refused.
    pub fn new(mut slots: Vec<Slot>) -> Result<Self, PriceError> {
        slots.sort_by_key(|slot| slot.t_start);
        let mut previous: Option<&Slot> = None;
        for slot in &slots {
            if !slot.is_finite() {
                return Err(PriceError::NonFinite {
                    t_start: slot.t_start,
                });
            }
            if DateTime::<Utc>::from_timestamp(slot.t_start, 0).is_none() {
                return Err(PriceError::Payload(format!(
                    "slot start {} is outside the representable range",
                    slot.t_start
                )));
            }
            let step = slot.source.slot_seconds();
            if slot.t_start.rem_euclid(step) != 0 {
                return Err(PriceError::Boundary {
                    t_start: slot.t_start,
                    expected: step,
                });
            }
            if let Some(prev) = previous {
                if prev.t_start == slot.t_start {
                    return Err(PriceError::Duplicate {
                        t_start: slot.t_start,
                    });
                }
                if prev.source != slot.source {
                    return Err(PriceError::Payload(format!(
                        "mixed resolutions: {} and {}",
                        prev.source, slot.source
                    )));
                }
                if slot.t_start != prev.t_end() {
                    return Err(PriceError::Gap {
                        t_start: prev.t_end(),
                    });
                }
            }
            previous = Some(slot);
        }
        Ok(Self { slots })
    }

    /// Slots in ascending start order.
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// Iterate in ascending start order.
    pub fn iter(&self) -> std::slice::Iter<'_, Slot> {
        self.slots.iter()
    }

    /// Number of slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the book holds no slots.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// First slot, if any.
    pub fn first(&self) -> Option<&Slot> {
        self.slots.first()
    }

    /// Last slot, if any.
    pub fn last(&self) -> Option<&Slot> {
        self.slots.last()
    }

    /// Exclusive end instant of the coverage, if the book is not empty.
    pub fn coverage_end(&self) -> Option<i64> {
        self.last().map(Slot::t_end)
    }

    /// The slot containing instant `t` (UTC epoch seconds).
    pub fn slot_at(&self, t: i64) -> Option<&Slot> {
        let next = self.slots.partition_point(|slot| slot.t_start <= t);
        if next == 0 {
            return None;
        }
        let candidate = self.slots.get(next - 1)?;
        (t < candidate.t_end()).then_some(candidate)
    }

    /// The published all-in price covering instant `t`.
    pub fn buy_at(&self, t: i64) -> Option<f64> {
        self.slot_at(t).map(|slot| slot.buy_all_in)
    }

    /// Slots starting in `[from, to)`.
    pub fn range(&self, from: i64, to: i64) -> impl Iterator<Item = &Slot> {
        self.slots
            .iter()
            .filter(move |slot| slot.t_start >= from && slot.t_start < to)
    }

    /// Require a slot covering `now` and coverage through `now + horizon_secs`.
    pub fn check_coverage(&self, now: i64, horizon_secs: i64) -> Result<(), PriceError> {
        let covered_until = self.coverage_end().unwrap_or(i64::MIN);
        let needed_until = now.saturating_add(horizon_secs);
        if self.slot_at(now).is_none() || covered_until < needed_until {
            return Err(PriceError::Stale {
                covered_until,
                needed_until,
            });
        }
        Ok(())
    }
}
