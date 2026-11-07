//! The API spelling of an engine override action.

use energy_core::dispatch::OverrideAction;

/// `charge` / `hold` / `off`, the wire spelling (the engine's own slug is for logs).
pub(crate) fn action_name(action: OverrideAction) -> &'static str {
    match action {
        OverrideAction::Charge { .. } => "charge",
        OverrideAction::Hold => "hold",
        OverrideAction::Off => "off",
    }
}
