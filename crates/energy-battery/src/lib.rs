//! Battery fleet: discovery, control, learning and protection.
//!
//! Responsibility (PLAN.md §2, §5): own the SolarFlow 800 Plus packs over the local zenSDK HTTP
//! API. No cloud, no token.
//!
//! * [`zensdk`] — `GET /properties/report` and `POST /properties/write`, one command at a time and
//!   at least 2 s apart per device. A write only counts as applied when the next report echoes the
//!   limit; HTTP 200 means nothing on its own.
//! * [`discovery`] — mDNS (`_zendure._tcp`) plus manual IP + SN entry for the cases mDNS misses.
//! * [`learning`] — per-pack capacity and round-trip efficiency from charge/discharge sessions,
//!   heavily recency-weighted, energy-balance form, trimmed outliers, bounded to a ±25 % band
//!   around the label; every estimate keeps a provenance row.
//! * [`protection`] — latching low-cell guard (persisted across restarts, stale reading keeps the
//!   latch) and the weekly full-charge maintenance.
//!
//! Control writes never touch `acMode` on a hold and always set `smartMode: 1`.

pub mod discovery;
pub mod learning;
pub mod protection;
pub mod zensdk;
