// Test module: assertions fail by panicking and `unwrap` is the intended way to express the
// expected value, so the shipped-code panic policy does not apply here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use super::{Config, ReconnectPolicy};

#[test]
fn debug_redacts_token() {
    let config = Config::direct("http://homeassistant:8123", "super-secret-token").unwrap();
    let debug = format!("{config:?}");
    assert!(!debug.contains("super-secret-token"));
    assert!(debug.contains("<redacted>"));
    assert_eq!(
        config.redact("Bearer super-secret-token"),
        "Bearer [REDACTED]"
    );
}

#[test]
fn direct_urls_are_derived() {
    let config = Config::direct("https://ha.example:8443/", "token").unwrap();
    assert_eq!(config.rest_base(), "https://ha.example:8443/api");
    assert_eq!(config.ws_url(), "wss://ha.example:8443/api/websocket");
}

#[test]
fn backoff_is_capped() {
    let policy = ReconnectPolicy {
        initial: Duration::from_millis(10),
        max: Duration::from_millis(50),
        jitter: false,
    };
    assert_eq!(policy.delay(0), Duration::from_millis(10));
    assert_eq!(policy.delay(2), Duration::from_millis(40));
    assert_eq!(policy.delay(20), Duration::from_millis(50));
}
