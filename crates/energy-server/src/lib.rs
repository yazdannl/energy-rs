//! Add-on entrypoint and HTTP surface.
//!
//! Responsibility (PLAN.md §2, §9, ha-apis.md §5.2): serve the versioned API, the SSE stream and the
//! embedded Svelte UI from one axum server on `0.0.0.0:8099`.
//!
//! * [`api`] — `/api/v1/...` endpoints. P0 only ships `GET /api/v1/health`; P8 adds plans, prices,
//!   batteries and savings.
//! * [`app`] — router assembly: the API under its prefix and the embedded assets as fallback.
//! * [`assets`] — `rust-embed`ed `web/dist` with an index.html fallback so client-side routes work
//!   behind ingress (relative paths only).
//!
//! The server is deliberately thin: it reads state, it does not decide anything.

pub mod api;
pub mod app;
pub mod assets;
