// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

use energy_battery::{
    DiscoveredDevice, DiscoveryRegistry, DiscoverySource, build_hostname, parse_hostname,
};
use support::SN;

mod support;

#[test]
fn parses_the_vendor_hostname() {
    let (product, sn) = parse_hostname("zendure-solarFlow800Plus-EOC1NLN9N4XXXXXX.local").unwrap();
    assert_eq!(product, "solarflow800plus");
    assert_eq!(sn, SN);
}

#[test]
fn accepts_field_spellings_of_the_hostname() {
    for host in [
        "Zendure-solarFlow800Plus-EOC1NLN9N4XXXXXX.local.",
        "zendure_solarFlow800Plus_EOC1NLN9N4XXXXXX",
        "zendure-solarFlow800Plus-EOC1NLN9N4XXXXXX",
    ] {
        let (_, sn) = parse_hostname(host).unwrap_or_else(|| panic!("{host}"));
        assert_eq!(sn, SN);
    }
}

#[test]
fn rejects_names_that_are_not_a_battery() {
    for host in [
        "zendure.local",
        "_zendure._tcp.local",
        "zendure-solarFlow800Plus.local",
        "zendure-meter-3ct",
        "zendure-smartmeter-ab",
        "other-solarFlow800Plus-EOC1NLN9N4XXXXXX.local",
        "",
    ] {
        assert_eq!(parse_hostname(host), None, "{host}");
    }
}

#[test]
fn builds_the_hostname_the_device_advertises() {
    assert_eq!(
        build_hostname("solarFlow 800 Plus", SN),
        "zendure-solarflow800plus-EOC1NLN9N4XXXXXX.local"
    );
    let (product, sn) = parse_hostname(&build_hostname("solarFlow800Plus", SN)).unwrap();
    assert_eq!(product, "solarflow800plus");
    assert_eq!(sn, SN);
}

#[test]
fn mdns_and_manual_entries_dedupe_by_serial() {
    let mut registry = DiscoveryRegistry::new();
    assert!(registry.merge(DiscoveredDevice::mdns(
        SN,
        "192.168.1.31",
        80,
        Some("solarflow800plus".to_owned())
    )));
    assert!(!registry.merge(DiscoveredDevice::mdns(SN, "192.168.1.31", 80, None)));
    assert_eq!(registry.len(), 1);

    assert!(registry.merge(DiscoveredDevice::manual("192.168.1.40", SN, 8080)));
    assert_eq!(registry.len(), 1);
    let merged = registry.get(SN).unwrap();
    assert_eq!(merged.host, "192.168.1.40");
    assert_eq!(merged.port, 8080);
    assert_eq!(merged.source, DiscoverySource::Manual);
    assert_eq!(merged.product.as_deref(), Some("solarflow800plus"));
}

#[test]
fn a_manual_entry_keeps_its_address_when_mdns_repeats() {
    let mut registry = DiscoveryRegistry::new();
    registry.merge(DiscoveredDevice::manual("192.168.1.40", SN, 80));
    let learned = registry.merge(DiscoveredDevice::mdns(
        SN,
        "192.168.1.31",
        80,
        Some("solarflow800plus".to_owned()),
    ));
    assert!(learned, "the model hint is new information");
    let kept = registry.get(SN).unwrap();
    assert_eq!(kept.host, "192.168.1.40");
    assert_eq!(kept.source, DiscoverySource::Manual);
    assert_eq!(kept.product.as_deref(), Some("solarflow800plus"));
    assert!(!registry.merge(DiscoveredDevice::mdns(SN, "192.168.1.31", 80, None)));
}

#[test]
fn a_repeat_announcement_follows_a_new_address() {
    let mut registry = DiscoveryRegistry::new();
    registry.merge(DiscoveredDevice::mdns(
        SN,
        "192.168.1.31",
        80,
        Some("solarflow800plus".to_owned()),
    ));
    assert!(registry.merge(DiscoveredDevice::mdns(SN, "192.168.1.77", 80, None)));
    let device = registry.get(SN).unwrap();
    assert_eq!(device.host, "192.168.1.77");
    assert_eq!(device.product.as_deref(), Some("solarflow800plus"));
    assert!(!registry.merge(DiscoveredDevice::mdns(SN, "192.168.1.77", 80, None)));
}

#[test]
fn serials_are_case_insensitive_and_the_endpoint_matches() {
    let mut registry = DiscoveryRegistry::new();
    registry.merge(DiscoveredDevice::mdns(
        SN.to_lowercase(),
        "192.168.1.31",
        80,
        None,
    ));
    registry.merge(DiscoveredDevice::manual("192.168.1.40", SN, 80));
    assert_eq!(registry.len(), 1);
    let device = registry.get(SN).unwrap();
    assert_eq!(device.sn, SN);
    let endpoint = device.endpoint();
    assert_eq!(endpoint.sn(), SN);
    assert_eq!(endpoint.port(), 80);
    assert_eq!(endpoint.host(), "192.168.1.40");
}

#[test]
fn many_devices_merge_into_one_entry_each() {
    let mut registry = DiscoveryRegistry::new();
    let changed = registry.merge_all([
        DiscoveredDevice::mdns(SN, "192.168.1.31", 80, None),
        DiscoveredDevice::mdns(SN, "192.168.1.31", 80, None),
        DiscoveredDevice::manual("192.168.1.40", "EOC1NLN9N4YYYYYY", 80),
    ]);
    assert_eq!(changed, 2);
    assert_eq!(registry.len(), 2);
    assert_eq!(registry.into_devices().len(), 2);
}
