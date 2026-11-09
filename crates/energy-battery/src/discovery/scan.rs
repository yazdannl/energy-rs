//! LAN scan for zenSDK devices: no mDNS, no vendor app.
//!
//! The packs answer `GET /properties/report` on port 80 and do not reliably advertise
//! `_zendure._tcp` (multicast is often filtered), so discovery also sweeps the local /24: a fast TCP
//! connect, then one report request per responder. A device that answers with its serial and a
//! SolarFlow/Hyper product is a battery; anything else is left alone.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream, UdpSocket};

use crate::discovery::found::DiscoveredDevice;
use crate::error::BatteryError;

/// How many hosts are probed at once.
const CONCURRENCY: usize = 64;
/// TCP connect budget per host: a device on the LAN answers in milliseconds.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
/// Whole-report budget once a host accepted the connection.
const READ_TIMEOUT: Duration = Duration::from_millis(1500);
/// Port every zenSDK device listens on.
const PORT: u16 = 80;

/// Sweep the local /24 around the primary IPv4 address.
///
/// `window` bounds the whole sweep; a shorter window simply probes fewer hosts. Returns every device
/// whose report carries a serial and a Zendure product string.
pub async fn scan_lan(window: Duration) -> Result<Vec<DiscoveredDevice>, BatteryError> {
    let Some(local) = local_ipv4().await else {
        return Err(BatteryError::Discovery {
            reason: "no local IPv4 address to scan from".to_owned(),
        });
    };
    let base = u32::from(local) & 0xffff_ff00;
    let deadline = tokio::time::Instant::now() + window;
    let mut found = Vec::new();
    let mut hosts = (1..=254u32)
        .map(|last| Ipv4Addr::from(base | last))
        .filter(|address| *address != local);
    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        let batch: Vec<Ipv4Addr> = hosts.by_ref().take(CONCURRENCY).collect();
        if batch.is_empty() {
            break;
        }
        let mut tasks = Vec::with_capacity(batch.len());
        for address in batch {
            tasks.push(tokio::spawn(probe(address)));
        }
        for task in tasks {
            if let Ok(Some(device)) = task.await {
                found.push(device);
            }
        }
    }
    Ok(found)
}

/// The primary local IPv4 address, from a connected UDP socket (no packet is sent).
async fn local_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;
    socket.connect("1.1.1.1:53").await.ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(address) if !address.is_loopback() => Some(address),
        _ => None,
    }
}

/// Connect, fetch the report and decide whether this address is a battery.
async fn probe(address: Ipv4Addr) -> Option<DiscoveredDevice> {
    let socket = TcpSocket::new_v4().ok()?;
    let stream = tokio::time::timeout(
        CONNECT_TIMEOUT,
        socket.connect(SocketAddr::new(IpAddr::V4(address), PORT)),
    )
    .await
    .ok()?
    .ok()?;
    let body = report_body(stream, address).await?;
    device_from_report(&body, address)
}

/// One `GET /properties/report`, returning the JSON body.
async fn report_body(mut stream: TcpStream, address: Ipv4Addr) -> Option<String> {
    let request =
        format!("GET /properties/report HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    tokio::time::timeout(READ_TIMEOUT, stream.write_all(request.as_bytes()))
        .await
        .ok()?
        .ok()?;
    let mut raw = Vec::new();
    tokio::time::timeout(READ_TIMEOUT, stream.read_to_end(&mut raw))
        .await
        .ok()?
        .ok()?;
    let text = String::from_utf8(raw).ok()?;
    let (head, body) = text.split_once("\r\n\r\n")?;
    if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
        return None;
    }
    Some(body.to_owned())
}

/// Accept only a report that names a serial and a Zendure battery product.
fn device_from_report(body: &str, address: Ipv4Addr) -> Option<DiscoveredDevice> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let sn = value.get("sn")?.as_str()?.trim();
    if sn.is_empty() {
        return None;
    }
    let product = value
        .get("product")
        .and_then(serde_json::Value::as_str)
        .map(str::to_lowercase);
    let is_battery = product
        .as_deref()
        .is_some_and(|product| product.contains("solarflow") || product.contains("hyper"));
    is_battery.then(|| DiscoveredDevice::scan(sn, address.to_string(), PORT, product))
}
