//! Where a discovered device came from.

/// Discovery provenance. A manual entry is an explicit operator decision and outranks mDNS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverySource {
    Mdns,
    /// Found by sweeping the LAN for a zenSDK report.
    Scan,
    Manual,
}

impl DiscoverySource {
    /// Higher wins when two entries describe the same serial and disagree on the address.
    pub const fn priority(self) -> u8 {
        match self {
            Self::Mdns => 0,
            Self::Scan => 1,
            Self::Manual => 2,
        }
    }
}
