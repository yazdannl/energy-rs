//! Report parsing and unit decoding.
//!
//! Two envelopes exist in the field: packs at the top level, or nested under `properties`. Both are
//! accepted, unknown keys are ignored, and every raw integer goes through [`decode`] so the unit
//! rules live in one place. There is no kWh counter in a report; pack capacity is a label derived
//! from the pack serial prefix and `packType`.

pub mod decode;
pub mod device_report;
pub mod grid_reverse;
pub mod pack;
pub mod pack_label;
pub mod parse;
pub mod properties;
pub mod soc_limit;

pub use device_report::Report;
pub use grid_reverse::GridReverse;
pub use pack::PackReport;
pub use pack_label::PackLabel;
pub use parse::parse_report;
pub use properties::Properties;
pub use soc_limit::SocLimit;
