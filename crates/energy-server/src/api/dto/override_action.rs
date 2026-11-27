//! The API spelling of an engine override action.

use energy_core::dispatch::OverrideAction;

/// `charge` / `discharge` / `hold` / `off`, the wire spelling (engine slugs are for logs).
pub(crate) fn action_name(action: OverrideAction) -> &'static str {
    match action {
        OverrideAction::Charge { .. } => "charge",
        OverrideAction::Discharge { .. } => "discharge",
        OverrideAction::Hold => "hold",
        OverrideAction::Off => "off",
    }
}
