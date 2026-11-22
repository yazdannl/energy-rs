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
//! * samples are weighted by **recency and span quality**, combined with a weighted median after a
//!   median-absolute-deviation trim, and their information is the covered SoC points discounted for
//!   whole-percent quantisation — session count alone earns no certainty;
//! * the published value is a **shrunk posterior**: the label (or the pack's own report), or the
//!   robust estimate pooled across identical-label packs, moved toward this pack's measurement in
//!   proportion to that information. Only strong long-span evidence can open the wider aging band
//!   and let one pack deviate; a reported capacity is always a hard ceiling.
//!
//! Before three closed cycles exist the typed default split converts the AC energy, so an early
//! capacity can be biased by the difference between the typed and the real efficiency. Shrinkage,
//! the tight band and the reported-capacity ceiling keep that bias out of the planner number; the
//! report names the source and the evidence behind every published value.
//!
//! Everything here is pure. Input timestamps and the report's `now` are arguments, the profile is
//! persisted through [`LearningStore`], and nothing reads a clock.

mod blocks;
pub mod bounds;
pub mod capacity;
pub mod cycle;
pub mod decision;
pub mod efficiency;
pub mod pair_attempt;
pub mod pair_diagnostics;
pub mod pair_refusal;
mod pairing;
pub mod pool;
pub mod profile;
pub mod refusal;
pub mod relearn;
pub mod report;
mod robust;
pub mod sample;
pub mod session;
pub mod source;
pub mod store;
pub mod weight;

pub use bounds::{
    AGING_CONFIDENCE, AGING_FRACTION, BAND_FRACTION, CapacityBounds, UPWARD_FRACTION,
};
pub use capacity::{
    CapacityEstimate, MAD_LIMIT, MAX_PLAUSIBLE_KWH, MIN_PLAUSIBLE_KWH, MIN_SESSIONS,
};
pub use cycle::CyclePair;
pub use decision::LearningDecision;
pub use efficiency::{
    AGREEMENT_BAND, CYCLE_TOLERANCE_FRACTION, CYCLE_TOLERANCE_PCT, DEFAULT_ROUND_TRIP,
    EfficiencyEstimate, HOLD_DRIFT_FRACTION, HOLD_DRIFT_PCT, MAX_IMPLIED_CAPACITY_KWH,
    MAX_PAIR_GAP_DAYS, MAX_ROUND_TRIP, MIN_CONFIDENCE, MIN_IMPLIED_CAPACITY_KWH, MIN_PAIRS,
    MIN_ROUND_TRIP,
};
pub use pair_attempt::PairAttempt;
pub use pair_diagnostics::PairDiagnostics;
pub use pair_refusal::PairRefusal;
pub use pool::CapacityPool;
pub use profile::LearningProfile;
pub use refusal::Refusal;
pub use relearn::{relearn, relearn_with_pool};
pub use report::LearningReport;
pub use sample::{
    CapacitySample, FULL_SPAN_PCT, INFO_SCALE_POINTS, MIN_USEFUL_SPAN_PCT, span_quality,
};
pub use session::{MIN_SESSION_SAMPLES, SessionRecord};
pub use source::CapacitySource;
pub use store::LearningStore;
pub use weight::{HALF_LIFE_SESSIONS, SessionWeight, recency_weight};
