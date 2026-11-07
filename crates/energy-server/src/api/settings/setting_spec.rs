//! One entry of the writable-settings schema.

use super::setting_kind::SettingKind;

/// A setting the API exposes: its ledger key, type and value when unset.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SettingSpec {
    /// Ledger `settings` key (also the API field name).
    pub(crate) key: &'static str,
    /// Value type and validation.
    pub(crate) kind: SettingKind,
    /// Text stored value when the key has never been written.
    pub(crate) default: &'static str,
}
