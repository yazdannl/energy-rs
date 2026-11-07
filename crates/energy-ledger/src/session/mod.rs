//! Session extraction: contiguous same-direction charge/discharge runs with their AC energy.

mod direction;
mod extract;
mod repo;
mod report;
mod row;
mod rules;
mod scan;

pub use direction::Direction;
pub use repo::Sessions;
pub use report::ExtractReport;
pub use row::Session;
pub use rules::SessionRules;
