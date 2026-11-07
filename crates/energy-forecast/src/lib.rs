//! PV and load forecasting, ported bit-exactly from the shipped Python add-on.
//!
//! Responsibility (PLAN.md §2 and §7, `reports/energy-rs/forecast-port-spec.md`):
//!
//! * [`model`] — the `pv_base.json` artifact: serde types that keep thresholds and leaf values as
//!   `u64` bit patterns, the frozen 28-name contract (digest-checked) and a loader that refuses a
//!   model this build does not serve.
//! * [`gbt`] — the exact tree walk: `<=` splits, per-node NaN routing, `is_leaf` as the only leaf
//!   test, leaves summed onto the baseline in artifact order, clamp applied to the sum.
//! * [`physics`] — two deliberately separate physics paths: Haurwitz/Erbs/NOCT/inverter for the
//!   learned model (UTC geometry), and Haurwitz/25%-diffuse for the capacity source (local clock).
//! * [`features`] — the 28 features in frozen order, built exactly as `pv.feature_vector` builds
//!   them, plus the night gate (`elevation <= 0` is zero production).
//! * [`bias`] — the 24-local-hour EWMA correction (alpha 0.2, 14-day decay) and the promoted nightly
//!   head that replaces it, never multiplied with it.
//! * [`load`] — the 48-bucket weekday/weekend profile, level scale, 7-day bias, temperature term
//!   (`r2 >= 0.05`), named-device subtraction and 15-minute serving.
//! * [`solar`] — source interpolation, weighted merge, capacity/derived physics, HA Energy series
//!   and per-source state.
//! * [`persistence`] — the schema-versioned learning payload store trait and its in-memory impl.
//!
//! The hard gate is the 800 golden vectors: every ratio must compare equal bit for bit. f64
//! everywhere, and no `unwrap`/`expect`/panic in library code.

#![forbid(unsafe_code)]

pub mod bias;
pub mod digest;
pub mod error;
pub mod features;
pub mod gbt;
pub mod load;
pub mod model;
pub mod persistence;
pub mod physics;
pub mod pv;
pub mod solar;
pub mod stats;
pub mod time;

pub use error::ForecastError;
pub use features::Features;
pub use model::PvModel;
pub use pv::{SlotEstimate, SlotInputs, estimate_slot};
