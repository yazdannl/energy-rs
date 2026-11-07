//! The engine: one task that owns every control decision.
//!
//! Responsibility (PLAN.md §2, §3, §6): drive the tick loop and be the only place that decides what
//! the batteries should do.
//!
//! * [`plan`] — the 120-slot scenario builder, the solver call and the typed refusal.
//! * [`safety`] — the ordered per-pack clamp ladder and its reasons.
//! * [`allocation`] — the fleet setpoint split, before the per-pack ramp.
//! * [`replan`] — the triggers that invalidate a stored plan.
//! * [`dispatch`] — one tick: fleet and meter read, plan slot interpolation, allocation, the
//!   fleet-level grid check, the per-pack ladder and the write through the command sink. The only
//!   code that talks to `energy-battery` writes.
//! * [`engine`] — the tokio loop, progress watchdog, persisted live-write latch, boot neutralisation,
//!   modes `advisory` -> `dry_run` -> `auto`, per-pack overrides and the shutdown hold.
//! * [`wiring`] — measurements, session extraction, capacity relearning, cell latch and the
//!   full-charge maintenance window.
//! * [`mode`] — the persisted control mode.
//! * [`stream`] — the contract with the HTTP server: the [`stream::EngineSnapshot`] the loop
//!   publishes after every tick, the [`stream::SnapshotPublisher`] that carries it, and the small
//!   set of [`stream::EngineCommand`]s the API may send back. The engine stays the only decider.
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
pub mod stream;
pub mod wiring;
