//! Per-pack capacity and round-trip efficiency, learned from the pack's own cycles.
//!
//! The rules come from the live fleet, not from a datasheet:
//!
//! * a capacity is **AC energy per SoC point**, taken from the charge side (a discharge drops a few
//!   more points than it gives back on a real BMS, so mixing directions produces two populations a
//!   few percent apart — that is a SoC-scale artifact, not a second capacity);
//! * the round trip comes from **closed cycles** only: a charge and the discharge that returns the
//!   pack to where the charge started. The split efficiency `x = eta_charge = eta_discharge` solves
//!   the energy balance `dSOC · C = I · x − O / x`; a closed cycle reduces to `x = sqrt(O / I)`;
//! * samples are weighted by **recency** (half-life three sessions: the last three dominate) and
//!   combined with a weighted median after a median-absolute-deviation trim;
//! * an estimate is applied only after **three** usable sessions, inside a ±25 % band around the
//!   pack label, and never above the capacity the pack reports. Below three, a confident previous
//!   profile is kept, never wiped.
//!
//! Before three closed cycles exist the typed default split converts the AC energy, so an early
//! capacity can be biased by the difference between the typed and the real efficiency. That is
//! exactly why the band and the reported-capacity ceiling exist: the first published number may be
//! replaced downward (aging), never upward past the label, and the report says which choice was
//! made with `LearningDecision`.
//!
//! Everything here is pure. Input timestamps and the report's `now` are arguments, the profile is
//! persisted through [`LearningStore`], and nothing reads a clock.

pub mod bounds;
pub mod capacity;
pub mod cycle;
pub mod decision;
pub mod efficiency;
pub mod profile;
pub mod refusal;
pub mod relearn;
pub mod report;
mod robust;
pub mod session;
pub mod store;
pub mod weight;

pub use bounds::{BAND_FRACTION, CapacityBounds};
pub use capacity::{
    CapacityEstimate, MAD_LIMIT, MAX_PLAUSIBLE_KWH, MIN_PLAUSIBLE_KWH, MIN_SESSIONS,
};
pub use cycle::CyclePair;
pub use decision::LearningDecision;
pub use efficiency::{
    CYCLE_TOLERANCE_FRACTION, CYCLE_TOLERANCE_PCT, DEFAULT_ROUND_TRIP, EfficiencyEstimate,
    HOLD_DRIFT_FRACTION, HOLD_DRIFT_PCT, MAX_IMPLIED_CAPACITY_KWH, MAX_PAIR_GAP_DAYS,
    MAX_ROUND_TRIP, MIN_IMPLIED_CAPACITY_KWH, MIN_PAIRS, MIN_ROUND_TRIP,
};
pub use profile::LearningProfile;
pub use refusal::Refusal;
pub use relearn::relearn;
pub use report::LearningReport;
pub use session::{MIN_SESSION_SAMPLES, SessionRecord};
pub use store::LearningStore;
pub use weight::{HALF_LIFE_SESSIONS, SessionWeight, recency_weight};
