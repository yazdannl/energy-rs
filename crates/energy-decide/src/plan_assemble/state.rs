//! Plan states and their dispatch permissions.

use crate::plan_assemble::DispatchPermission;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanState {
    /// Hard constraints satisfied and all required outcomes met.
    Healthy,
    /// Hard constraints satisfied, optional obligations unmet inside an accepted scope.
    DegradedAccepted,
    /// A hard constraint or required outcome was not met.
    Infeasible,
}

impl PlanState {
    /// Dispatch permission attached to this state.
    pub const fn dispatch_permission(self) -> DispatchPermission {
        match self {
            Self::Healthy => DispatchPermission::EconomicAndSafety,
            Self::DegradedAccepted => DispatchPermission::ScopedEconomicAndSafety,
            Self::Infeasible => DispatchPermission::SafetyOnly,
        }
    }
}
