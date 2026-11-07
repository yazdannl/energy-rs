//! The 24-local-hour multiplicative correction and the promoted head that replaces it.

mod correction;
mod decay;
mod head_fit;
mod head_profile;
mod head_report;
mod head_sample;
mod head_status;
mod hourly;
mod sample;

pub use correction::Correction;
pub use decay::{BIAS_ALPHA, BIAS_DECAY_DAYS, BIAS_MAX, BIAS_MIN, MIN_FORECAST_KW, decay};
pub use head_fit::{FACTOR_MAX, FACTOR_MIN, SHRINKAGE, fit_factors};
pub use head_profile::HeadProfile;
pub use head_report::{HeadReport, evaluate, mean_absolute_error};
pub use head_sample::HeadSample;
pub use head_status::HeadStatus;
pub use hourly::HourlyBias;
pub use sample::BiasSample;
