//! Disjoint write classes: device commands and control-plane records.

/// The class of a write request. A device class can send to a pack; no other class can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteClass {
    /// Ordinary economic setpoint, including holds produced downstream by safety.
    DeviceEconomic,
    /// Safety-lane neutralisation carrying a fence that must be fresh at execution.
    DeviceNeutralisation { fence_id: String },
    /// Authority token, generation, transition or clearance journal entry.
    ControlPlaneAuthority,
    /// Non-authoritative operator/attempt journal entry.
    ControlPlaneJournal,
    /// Learning, maintenance, dashboard or non-authority attempt persistence.
    DomainPersistence,
    /// Append-only startup-resolution diagnostic record; writable in every phase.
    StartupResolution,
    /// Operational request to enter dry_run; always rejected.
    OperationalDryRun,
    /// Clearance command validation; the boolean is the only payload.
    ClearanceAuthorization { valid: bool },
    /// Ordinary append trying to write or replace the authority checkpoint.
    CheckpointWrite,
}

impl WriteClass {
    /// True only for classes that may reach a device.
    pub fn is_device(&self) -> bool {
        matches!(
            self,
            Self::DeviceEconomic | Self::DeviceNeutralisation { .. }
        )
    }
}
