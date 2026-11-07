//! Add-on entrypoint and HTTP surface.
//!
//! Responsibility (PLAN.md §2, §9, ha-apis.md §5.2): serve the versioned API, the SSE stream and the
//! embedded Svelte UI from one axum server on `0.0.0.0:8099`.
//!
//! * [`service`] — the read model: ledger handle, price repository, the engine's published
//!   [`EngineSnapshot`](energy_core::stream::EngineSnapshot) and the command queue back to the
//!   engine.
//! * [`api`] — `/api/v1/...` endpoints, their DTOs and the one error envelope.
//! * [`app`] — router assembly: the API under its prefix and the embedded assets as fallback.
//! * [`assets`] — `rust-embed`ed `web/dist` with an index.html fallback for the SPA routes so
//!   client-side navigation works behind ingress (relative paths only).
//!
//! The server is deliberately thin: it reads state and validates requests, it does not decide
//! anything. Control decisions belong to `energy-core`; the contract between the two is
//! [`energy_core::stream`].

pub mod api;
pub mod app;
pub mod assets;
pub mod bootstrap;
pub mod engine_handles;
pub mod service;
pub mod service_options;

pub use energy_core::safety::Authority;
pub use energy_core::stream;
pub use engine_handles::EngineHandles;
pub use service::Service;
pub use service_options::ServiceOptions;
