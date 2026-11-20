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
//! * The guard needs a finite meter reading and a fresh, available fleet. A missing/stale reading
//!   changes nothing -- the plan stands, and no discharge is guessed.
//! * `excess = measured_grid_w - target_import`. With `deadband = grid_follow_deadband_w`:
//!   - `excess > deadband`: arm and ask the fleet for `measured_battery_w - excess` W. Adding that
//!     change to the battery's *measured* output is what brings the meter down by `excess`, without
//!     integrating last tick's correction into this one.
//!   - `-deadband <= excess <= deadband` while armed: hold the measured battery output. This is
//!     the hysteresis: the guard reuses the grid-follow deadband (no separate setting), so a pack
//!     that just brought the meter to the target is not released on the very next tick and made to
//!     flap between the plan and the ceiling.
//!   - `excess < -deadband`: disarm and execute the plan again. Before the first arm, a reading
//!     inside the band is the plan's business and leaves the setpoint untouched.
//! * The requested setpoint is then `min(plan_w, guard_w)`: only a move towards discharge survives;
//!   a grid-charge slot is reduced or stopped, but a charge is never started by this module. The
//!   result is capped by the fleet's usable discharge capability, and every pack bound (SoC floor,
//!   cell latch, ramp, fleet/site ceilings) still applies downstream.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::dispatch::{FleetView, MeterReading, PackReading};
use crate::safety::{EPS_W, SafetyLimits};

/// The guard's answer for one tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ceiling {
    /// The fleet setpoint after the guard, W.
    pub(crate) target_w: f64,
    /// Whether the guard actually lowered the plan's setpoint (for the dispatch reason).
    pub(crate) active: bool,
}

/// The reactive ceiling's hysteresis memory, owned by the dispatcher.
///
/// One bit for the whole fleet: the guard is armed by a measured breach and holds its discharge
/// until the meter is clearly back below the target. An `AtomicBool` keeps the dispatcher's `tick`
/// signature unchanged, exactly as the fault backoff does.
#[derive(Debug, Default)]
pub(crate) struct Guard {
    armed: AtomicBool,
}

impl Guard {
    /// Fold the reactive ceiling into the plan/follow target for this tick.
    pub(crate) fn apply(
        &self,
        plan_w: f64,
        meter: &MeterReading,
        fleet: &FleetView,
        limits: &SafetyLimits,
        stale: bool,
    ) -> Ceiling {
        let off = Ceiling {
            target_w: plan_w,
            active: false,
        };
        if stale || !plan_w.is_finite() || !limits.max_grid_import_w.is_finite() {
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
        let deadband_w = finite_nonnegative(limits.grid_follow_deadband_w);
        let excess = grid_w - target_import;
        let armed = self.armed.load(Ordering::SeqCst);
        if excess < -deadband_w {
            self.disarm();
            return off;
        }
        let breached = excess > deadband_w;
        if !armed && !breached {
            return off;
        }
        if breached {
            self.armed.store(true, Ordering::SeqCst);
        }
        let guard_w = if breached {
            battery_w - excess
        } else {
            battery_w
        };
        // Cap the guard's own ask first, then lower the plan with it. Reversing these two steps
        // could *raise* a plan that already discharges more than this fleet can deliver.
        let target_w = plan_w.min(guard_w.max(-capability_w));
        Ceiling {
            target_w,
            active: target_w < plan_w - EPS_W,
        }
    }

    fn disarm(&self) {
        self.armed.store(false, Ordering::SeqCst);
    }
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
