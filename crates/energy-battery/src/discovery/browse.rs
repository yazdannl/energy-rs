//! mDNS browsing for `_zendure._tcp`.

use std::time::{Duration, Instant};

use mdns_sd::{Receiver, ResolvedService, ServiceDaemon, ServiceEvent};

use crate::discovery::found::DiscoveredDevice;
use crate::discovery::hostname::{SERVICE_TYPE, parse_hostname};
use crate::error::BatteryError;
use crate::transport::Endpoint;

/// Browse the LAN for `window` and return every battery that resolved.
///
/// Blocking mDNS work runs on a blocking thread so a slow multicast response never stalls the
/// runtime. Meters that advertise the same service are filtered out by the hostname pattern, and a
/// device without a usable name or address is simply absent from the result.
pub async fn browse_mdns(window: Duration) -> Result<Vec<DiscoveredDevice>, BatteryError> {
    tokio::task::spawn_blocking(move || browse_blocking(window))
        .await
        .map_err(|err| BatteryError::Discovery {
            reason: format!("mDNS task failed: {err}"),
        })?
}

fn browse_blocking(window: Duration) -> Result<Vec<DiscoveredDevice>, BatteryError> {
    let daemon = ServiceDaemon::new().map_err(|err| discovery_error(&err))?;
    let receiver = daemon
        .browse(SERVICE_TYPE)
        .map_err(|err| discovery_error(&err))?;
    let found = collect(&receiver, window);
    let _ = daemon.shutdown();
    Ok(found)
}

fn collect(receiver: &Receiver<ServiceEvent>, window: Duration) -> Vec<DiscoveredDevice> {
    let deadline = Instant::now() + window;
    let mut found = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(service)) => {
                if let Some(device) = from_service(&service) {
                    found.push(device);
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    found
}

fn from_service(service: &ResolvedService) -> Option<DiscoveredDevice> {
    let named =
        parse_hostname(service.get_hostname()).or_else(|| parse_hostname(service.get_fullname()));
    let (product, sn) = match named {
        Some((product, sn)) => (Some(product), sn),
        None => (txt_product(service), txt_serial(service)?),
    };
    let port = match service.get_port() {
        0 => Endpoint::DEFAULT_PORT,
        port => port,
    };
    Some(DiscoveredDevice::mdns(
        sn,
        reachable_host(service),
        port,
        product,
    ))
}

fn reachable_host(service: &ResolvedService) -> String {
    service.get_addresses_v4().into_iter().min().map_or_else(
        || service.get_hostname().trim_end_matches('.').to_owned(),
        |ip| ip.to_string(),
    )
}

fn txt_serial(service: &ResolvedService) -> Option<String> {
    service.get_property_val_str("sn").map(str::to_owned)
}

fn txt_product(service: &ResolvedService) -> Option<String> {
    service
        .get_property_val_str("product")
        .map(str::to_owned)
        .filter(|product| !product.is_empty())
}

fn discovery_error(err: &mdns_sd::Error) -> BatteryError {
    BatteryError::Discovery {
        reason: err.to_string(),
    }
}
