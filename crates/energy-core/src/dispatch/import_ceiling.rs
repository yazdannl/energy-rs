//! Reactive guard for the measured grid import, above the plan and the grid follow.
//!
//! The plan (and the grid-follow correction in [`super::follow_target`]) decide *why* the fleet
//! charges or serves. This guard decides only when the house is about to exceed the electrical
//! installation's import ceiling despite that decision. It is safety, not economics: it can lower
//! a setpoint, never raise one, and it does not touch the per-pack ladder, the fault backoff, the
//! write gate or the write pacer.
//!
//! # Control law
//!
//! * `limit = SafetyLimits::max_grid_import_w`; `limit <= 0` (or non-finite) disables the guard.
//! * `margin = max(0, grid_import_margin_w)`, so `margin = 0` holds exactly at the limit.
//! * `target_import = max(0, limit - margin)`: a margin wider than the limit targets zero import.
//! * The guard needs a finite meter reading and a fresh, available fleet. A missing/stale *meter*
//!   reading changes nothing -- the plan stands, and no discharge is guessed.
//! * The release prediction compares readings from one instant. The meter stamp and every pack
//!   `observed_at` must be within [`MAX_TELEMETRY_SKEW_S`] of the tick and of each other. When they
//!   are not, an armed guard holds the measured fleet output instead of releasing on skew.
//!   Plan availability is
//!   not telemetry freshness: with no executable plan the replacement target is neutral
//!   ([`NO_PLAN_TARGET_W`]), so the guard can still discharge protectively but never charge.
//! * `excess = measured_grid_w - target_import`. With `deadband = grid_follow_deadband_w`:
//!   - `excess > deadband`: arm and ask the fleet for `measured_battery_w - excess` W. Adding that
//!     change to the battery's *measured* output is what brings the meter down by `excess`, without
//!     integrating last tick's correction into this one.
//!   - armed and `excess <= deadband`: hold the measured battery output, unless releasing is safe.
//!   - Release is decided by the *replacement* command, not by the meter alone. If the guard let
//!     go, the pack would return to `replacement_w`, so the settled grid is predicted as
//!     `measured_grid_w + replacement_w - measured_battery_w`. Release only when that is at most
//!     `target_import - deadband`. A meter dip caused by our own discharge therefore cannot release
//!     the guard and recreate the overload; a genuine load drop, or a plan that already discharges
//!     enough, can. This is the hysteresis, and it reuses the grid-follow deadband (no new setting).
//! * The requested setpoint is then `min(replacement_w, guard_w)`: only a move towards discharge
//!   survives; a grid-charge slot is reduced or stopped, but a charge is never started. The guard's
//!   own ask is capped by the fleet's usable discharge capability, and every pack bound (SoC floor,
//!   cell latch, ramp, fleet/site ceilings) still applies downstream.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::dispatch::{FleetView, MeterReading, PackReading};
use crate::safety::{EPS_W, SafetyLimits};

/// Neutral replacement target when no plan can be executed: the guard may only lower it.
pub(crate) const NO_PLAN_TARGET_W: f64 = 0.0;

/// Largest accepted difference between the meter stamp, a pack stamp and the tick, seconds.
///
/// The tick is 5 s and the actuator ramp is 8-10 s: comparing a lagging battery report with a
/// fresh meter would predict a safe release that does not exist. Five seconds is one tick.
const MAX_TELEMETRY_SKEW_S: u64 = 5;

/// The guard's answer for one tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ceiling {
    /// The fleet setpoint after the guard, W.
    pub(crate) target_w: f64,
    /// Whether the guard actually lowered the replacement target (for the dispatch reason).
    pub(crate) active: bool,
}

impl Ceiling {
    const fn off(replacement_w: f64) -> Self {
        Self {
            target_w: replacement_w,
            active: false,
        }
    }
}

/// The reactive ceiling's hysteresis memory, owned by the dispatcher.
///
/// One bit for the whole fleet: the guard is armed by a measured breach and holds its discharge
/// until releasing would still keep the import under the ceiling. An `AtomicBool` keeps the
/// dispatcher's `tick` signature unchanged, exactly as the fault backoff does.
#[derive(Debug, Default)]
pub(crate) struct Guard {
    armed: AtomicBool,
}

impl Guard {
    /// Fold the reactive ceiling into the plan/follow target for this tick.
    ///
    /// `replacement_w` is what would run without the guard: the plan/follow setpoint when the plan
    /// is executable, otherwise [`NO_PLAN_TARGET_W`]. The guard never raises it.
    pub(crate) fn apply(
        &self,
        now: i64,
        replacement_w: f64,
        meter: &MeterReading,
        fleet: &FleetView,
        limits: &SafetyLimits,
    ) -> Ceiling {
        let off = Ceiling::off(replacement_w);
        if !replacement_w.is_finite() || !limits.max_grid_import_w.is_finite() {
            self.disarm();
            return off;
        }
        let limit = limits.max_grid_import_w;
        if limit <= 0.0 || !limits.grid_import_margin_w.is_finite() {
            self.disarm();
            return off;
        }
        let target_import = (limit - limits.grid_import_margin_w.max(0.0)).max(0.0);
        let Some(grid_w) = meter.grid_w.filter(|value| value.is_finite()) else {
            self.disarm();
            return off;
        };
        // The correction is anchored on the fleet's measured output, so an incomplete or stale
        // fleet read cannot be used. Nothing is guessed.
        if !fleet.packs.iter().all(PackReading::usable) {
            self.disarm();
            return off;
        }
        let battery_w = fleet.power_w();
        let capability_w = capability(fleet, limits);
        if !battery_w.is_finite() || capability_w <= 0.0 {
            self.disarm();
            return off;
        }
        if !aligned(now, meter, fleet) {
            return self.on_skew(replacement_w, battery_w, capability_w);
        }
        let deadband_w = finite_nonnegative(limits.grid_follow_deadband_w);
        let excess = grid_w - target_import;
        let breached = excess > deadband_w;
        if breached {
            self.armed.store(true, Ordering::SeqCst);
        } else {
            // Predict the settled grid if the pack returns to `replacement_w`.
            let settled_grid_w = grid_w + replacement_w - battery_w;
            if settled_grid_w <= target_import - deadband_w {
                self.disarm();
                return off;
            }
            if !self.armed.load(Ordering::SeqCst) {
                return off;
            }
        }
        let guard_w = if breached {
            battery_w - excess
        } else {
            battery_w
        };
        // Cap the guard's own ask first, then lower the replacement with it. Reversing these two
        // steps could *raise* a plan that already discharges more than this fleet can deliver.
        let target_w = replacement_w.min(guard_w.max(-capability_w));
        Ceiling {
            target_w,
            active: target_w < replacement_w - EPS_W,
        }
    }

    /// Skewed telemetry cannot prove a release. Hold the measured output while armed; an
    /// unarmed guard makes no guess at all.
    fn on_skew(&self, replacement_w: f64, battery_w: f64, capability_w: f64) -> Ceiling {
        if !self.armed.load(Ordering::SeqCst) {
            return Ceiling::off(replacement_w);
        }
        let target_w = replacement_w.min(battery_w.max(-capability_w));
        Ceiling {
            target_w,
            active: target_w < replacement_w - EPS_W,
        }
    }

    fn disarm(&self) {
        self.armed.store(false, Ordering::SeqCst);
    }
}

/// Whether the meter and every pack describe the same tick, within one tick of skew.
fn aligned(now: i64, meter: &MeterReading, fleet: &FleetView) -> bool {
    let Some(meter_at) = meter.observed_at else {
        return false;
    };
    if now.abs_diff(meter_at) > MAX_TELEMETRY_SKEW_S {
        return false;
    }
    fleet.packs.iter().all(|pack| {
        now.abs_diff(pack.observed_at) <= MAX_TELEMETRY_SKEW_S
            && meter_at.abs_diff(pack.observed_at) <= MAX_TELEMETRY_SKEW_S
    })
}

/// Discharge capability the guard may count on, W (0 when no pack can serve).
fn capability(fleet: &FleetView, limits: &SafetyLimits) -> f64 {
    let packs: f64 = fleet
        .packs
        .iter()
        .map(|pack| pack.spec.max_discharge_w)
        .filter(|watts| watts.is_finite() && *watts > 0.0)
        .sum();
    if limits.fleet_max_discharge_w > 0.0 {
        packs.min(limits.fleet_max_discharge_w)
    } else {
        packs
    }
}

fn finite_nonnegative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}
