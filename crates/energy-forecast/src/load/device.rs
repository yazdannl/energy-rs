//! One `load.devices` entry: a named load with its own meter.

use super::device_kind::DeviceKind;

/// The prefix of the store key, shared with the Python add-on so the two can run side by side.
pub const DEVICE_KEY_PREFIX: &str = "device:";

/// One configured named device.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadDevice {
    /// Store key: `device:<entity>`; survives a rename of the display name.
    pub key: String,
    /// The meter entity id.
    pub entity: String,
    /// The closed-set kind.
    pub kind: DeviceKind,
    /// Display name, defaulted per kind.
    pub name: String,
    /// Whether its profile gets a temperature term; derived from the kind, never configured.
    pub temperature: bool,
}

impl LoadDevice {
    /// Build a device from its meter and kind, defaulting the key and display name. An empty
    /// `name` takes the kind's label, or the entity id for an unknown kind.
    pub fn new(entity: &str, kind: &DeviceKind, name: &str) -> Self {
        let temperature = kind.is_temperature();
        let fallback = kind.label().unwrap_or(entity);
        Self {
            key: format!("{DEVICE_KEY_PREFIX}{entity}"),
            entity: entity.to_string(),
            kind: kind.clone(),
            name: if name.is_empty() {
                fallback.to_string()
            } else {
                name.to_string()
            },
            temperature,
        }
    }

    /// `name (kind)`, used in forecast notes.
    pub fn label(&self) -> String {
        format!("{} ({})", self.name, self.kind)
    }
}
