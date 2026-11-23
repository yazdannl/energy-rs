//! Whether an unmet obligation makes the whole plan infeasible.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObligationClass {
    /// Required outcome inside the horizon; a shortfall is infeasible.
    Required,
    /// Optional outcome (deadline outside horizon or ineligible pack).
    Optional,
}
