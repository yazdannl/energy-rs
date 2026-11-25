//! Capabilities deliberately absent from the sealed bootstrap transaction.

/// Capability an untrusted caller may try to smuggle into bootstrap; always denied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapCapability {
    /// Device command or setpoint.
    DeviceWrite,
    /// Journal entry unrelated to the token initialization.
    JournalAppend,
    /// Settings or mode mutation.
    SettingsWrite,
    /// Queue or schedule work.
    QueueWork,
}
