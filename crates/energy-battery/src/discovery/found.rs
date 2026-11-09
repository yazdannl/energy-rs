//! One discovered device.

use crate::discovery::source::DiscoverySource;
use crate::transport::Endpoint;

/// A device found on the LAN or entered by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredDevice {
    pub sn: String,
    /// Host or IPv4 address to reach the device on.
    pub host: String,
    /// Port from the mDNS SRV record, else 80.
    pub port: u16,
    /// Model hint from the hostname; display only, never a control decision.
    pub product: Option<String>,
    pub source: DiscoverySource,
}

impl DiscoveredDevice {
    /// An mDNS hit: host may be an IPv4 address or a `.local` hostname.
    pub fn mdns(
        sn: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        product: Option<String>,
    ) -> Self {
        Self {
            sn: normalize_sn(&sn.into()),
            host: host.into(),
            port,
            product,
            source: DiscoverySource::Mdns,
        }
    }

    /// A LAN-scan hit: a verified address that answered the zenSDK report.
    pub fn scan(
        sn: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        product: Option<String>,
    ) -> Self {
        Self {
            sn: normalize_sn(&sn.into()),
            host: host.into(),
            port,
            product,
            source: DiscoverySource::Scan,
        }
    }

    /// An operator-entered address and serial.
    pub fn manual(host: impl Into<String>, sn: impl Into<String>, port: u16) -> Self {
        Self {
            sn: normalize_sn(&sn.into()),
            host: host.into(),
            port,
            product: None,
            source: DiscoverySource::Manual,
        }
    }

    pub fn endpoint(&self) -> Endpoint {
        Endpoint::with_port(self.host.clone(), self.port, self.sn.clone())
    }
}

/// Serials are uppercase on the hardware; compare and dedupe case-insensitively.
fn normalize_sn(sn: &str) -> String {
    sn.trim().to_ascii_uppercase()
}
