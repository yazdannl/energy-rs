//! The writable property set, serialized in the order the device expects.

use serde::Serialize;

/// The only properties this client ever writes, in payload order: `smartMode`, `acMode`,
/// `outputLimit`, `inputLimit`.
///
/// Order is not cosmetic. `smartMode` goes first so the RAM-only flag is set before any value it
/// must protect is applied; `acMode` only appears when the direction changes; a hold carries no
/// `acMode` at all (it flaps `smartMode` on some firmware, and a hold is just both limits at 0).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProperties {
    #[serde(skip_serializing_if = "Option::is_none")]
    smart_mode: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ac_mode: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_limit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_limit: Option<i32>,
}

impl WriteProperties {
    /// Charge at `watts`: direction 1, the discharge limit zeroed in the same body.
    pub fn charge(watts: u32) -> Self {
        Self {
            smart_mode: Some(1),
            ac_mode: Some(1),
            output_limit: Some(0),
            input_limit: Some(to_wire(watts)),
        }
    }

    /// Discharge at `watts`: the mirror image of [`WriteProperties::charge`].
    pub fn discharge(watts: u32) -> Self {
        Self {
            smart_mode: Some(1),
            ac_mode: Some(2),
            output_limit: Some(to_wire(watts)),
            input_limit: Some(0),
        }
    }

    /// AC hold: both limits at 0, RAM only, `acMode` untouched. PV still charges the pack.
    pub fn hold() -> Self {
        Self {
            smart_mode: Some(1),
            ac_mode: None,
            output_limit: Some(0),
            input_limit: Some(0),
        }
    }

    /// Trim a running charge: only that limit changes, `acMode` and the other limit stay put.
    pub fn trim_charge(watts: u32) -> Self {
        Self {
            smart_mode: Some(1),
            ac_mode: None,
            output_limit: None,
            input_limit: Some(to_wire(watts)),
        }
    }

    /// Trim a running discharge.
    pub fn trim_discharge(watts: u32) -> Self {
        Self {
            smart_mode: Some(1),
            ac_mode: None,
            output_limit: Some(to_wire(watts)),
            input_limit: None,
        }
    }

    /// No property at all (builder base; never sent on its own).
    pub const fn empty() -> Self {
        Self {
            smart_mode: None,
            ac_mode: None,
            output_limit: None,
            input_limit: None,
        }
    }

    pub fn smart_mode(&self) -> Option<i32> {
        self.smart_mode
    }

    pub fn ac_mode(&self) -> Option<i32> {
        self.ac_mode
    }

    pub fn output_limit(&self) -> Option<i32> {
        self.output_limit
    }

    pub fn input_limit(&self) -> Option<i32> {
        self.input_limit
    }

    pub fn is_empty(&self) -> bool {
        self.smart_mode.is_none()
            && self.ac_mode.is_none()
            && self.output_limit.is_none()
            && self.input_limit.is_none()
    }
}

fn to_wire(watts: u32) -> i32 {
    i32::try_from(watts).unwrap_or(i32::MAX)
}
