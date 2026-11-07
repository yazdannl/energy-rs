//! Home Assistant integration.
//!
//! Responsibility (PLAN.md §2, §7): everything that talks to Core.
//!
//! * [`client`] — REST (`/api/...`) and WebSocket (`/api/websocket`) access, auth handshake,
//!   typed errors, timeouts and reconnect/restartable supervision.
//! * [`discovery`] — resolve the energy dashboard configuration (`energy/get_prefs`) into the
//!   grid/solar/load/battery sensor set this app needs.
//! * [`statistics`] — long-term statistics reads (`recorder/statistics_during_period`), using
//!   `change` (not `sum`) for energy counters.
//! * [`subscribe`] — live state subscriptions feeding the engine's slot measurements.
//!
//! In the add-on the base URL is `http://supervisor/core` with `SUPERVISOR_TOKEN`; the same code
//! works against a plain HA URL with a long-lived token for local development.

pub mod client;
pub mod discovery;
pub mod statistics;
pub mod subscribe;
