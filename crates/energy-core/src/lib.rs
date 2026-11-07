//! The engine: one task that owns every control decision.
//!
//! Responsibility (PLAN.md §2, §3, §6): drive the 15-minute loop and be the only place that decides
//! what the batteries should do.
//!
//! * [`plan`] — the 120-slot scenario builder, the solver call and the typed refusal.
//! * [`safety`] — the ordered per-pack clamp ladder and its reasons.
//! * [`allocation`] — the fleet setpoint split, before the per-pack ramp.
//! * [`replan`] — the triggers that invalidate a stored plan.
//! * [`engine`] — slot boundary handling, input assembly (prices, forecast, measurements, plan
//!   history), replanning the 120-slot horizon, watchdog and stale-input hold.
//!   The cell-voltage latch that survives restarts is `energy-battery::protection`; the ladder
//!   obeys it.
//! * [`dispatch`] — setpoint rules and plan-SoC tracking; the only code that talks to
//!   `energy-battery` writes.
//! * [`mode`] — `advisory` -> `dry_run` -> `auto`; the mode is a persisted setting and `dry_run` is
//!   the default. In `advisory`/`dry_run` the full pipeline runs and intended writes are logged but
//!   never sent.
//!
//! Layers below this crate are synchronous where possible; only HA, zenSDK, timers and SQLite are
//! async, and everything is supervised so a failure holds instead of guessing.

pub mod allocation;
pub mod dispatch;
pub mod engine;
pub mod mode;
pub mod plan;
pub mod replan;
pub mod safety;
