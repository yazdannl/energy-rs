//! Retention: the trim job and its policy.

mod job;
mod policy;
mod report;

pub use policy::RetentionPolicy;
pub use report::RetentionReport;

pub(crate) use job::run;
