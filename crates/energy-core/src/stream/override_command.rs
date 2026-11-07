//! A validated manual pack override.

use crate::dispatch::OverrideAction;

/// Pin one pack for `ttl_s` seconds.
///
/// `action` is the engine's own [`OverrideAction`], so the API cannot invent a fourth action and the
/// charge power travels inside the variant. The server clamps a charge request to the pack's limit
/// before building the command, so the engine receives a value it can honour.
#[derive(Debug, Clone, PartialEq)]
pub struct OverrideCommand {
    /// Pack serial number.
    pub pack: String,
    /// What to do with the pack.
    pub action: OverrideAction,
    /// How long the override stays in force, seconds.
    pub ttl_s: u32,
}
