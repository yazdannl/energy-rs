//! LAN discovery: mDNS plus manual entry, merged by serial number.
//!
//! There is no cloud call and no token: the device list is either multicast (`_zendure._tcp`, with
//! the hostname `zendure-<product>-<sn>.local`) or typed in by the operator. Identity is settled by
//! the report's `sn`; the mDNS name is a hint, and a meter that answers the same service is filtered
//! out because its hostname does not parse as a battery.

pub mod browse;
pub mod found;
pub mod hostname;
pub mod registry;
pub mod source;

pub use browse::browse_mdns;
pub use found::DiscoveredDevice;
pub use hostname::{SERVICE_TYPE, build_hostname, parse_hostname};
pub use registry::DiscoveryRegistry;
pub use source::DiscoverySource;
