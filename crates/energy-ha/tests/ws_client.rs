// Test code: a failed assertion is the report, so `unwrap`/`expect`/`panic!` are the intended
// API here.  The workspace policy (no panics in shipped code) applies to the library.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::float_cmp,
    clippy::manual_midpoint
)]

mod support;

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use energy_ha::{Config, Connection, EnergyPreferences, HaClient, HaError, ReconnectPolicy};
use serde_json::{Value, json};
use support::{FakeServer, TOKEN};

#[tokio::test]
async fn authenticated_calls_increment_ids_and_deserialize() {
    let server = FakeServer::spawn(Arc::new(|_, command, request| match command {
        "energy/get_prefs" => support::Reply::ok(json!({
            "energy_sources": [],
            "device_consumption": []
        })),
        "test/echo" => support::Reply::ok(json!({"value": request["value"].clone()})),
        _ => support::Reply::fail("unknown_command", "unexpected"),
    }))
    .await;
    let config = Config::direct(&server.base_url(), TOKEN).expect("config");
    let connection = Connection::connect(&config).await.expect("connect");
    assert_eq!(connection.ha_version(), "2026.10.4");

    let prefs: EnergyPreferences = connection
        .call("energy/get_prefs", Value::Null)
        .await
        .expect("prefs");
    assert!(prefs.energy_sources.is_empty());

    let first: Value = connection
        .call("test/echo", json!({"value": 1}))
        .await
        .expect("first");
    let second: Value = connection
        .call("test/echo", json!({"value": 2}))
        .await
        .expect("second");
    assert_eq!(first["value"], 1);
    assert_eq!(second["value"], 2);

    let ids = server.request_ids.lock().expect("ids").clone();
    assert_eq!(ids, vec![1, 2, 3]);
}

/// Home Assistant refuses a request whose id is not greater than the previous one (`id_reuse`), so
/// concurrent callers must not swap ids between assignment and enqueue.
#[tokio::test]
async fn concurrent_calls_keep_the_ids_in_order() {
    let server = FakeServer::spawn(Arc::new(|_, command, request| match command {
        "test/echo" => support::Reply::ok(json!({"value": request["value"].clone()})),
        _ => support::Reply::fail("unknown_command", "unexpected"),
    }))
    .await;
    let config = Config::direct(&server.base_url(), TOKEN).expect("config");
    let connection = Arc::new(Connection::connect(&config).await.expect("connect"));

    let mut tasks = Vec::new();
    for index in 0..16 {
        let connection = Arc::clone(&connection);
        tasks.push(tokio::spawn(async move {
            let echoed: Value = connection
                .call("test/echo", json!({"value": index}))
                .await
                .expect("echo");
            assert_eq!(echoed["value"], index);
        }));
    }
    for task in tasks {
        task.await.expect("join");
    }

    let ids = server.request_ids.lock().expect("ids").clone();
    assert_eq!(ids.len(), 16);
    assert!(
        ids.windows(2).all(|pair| pair[0] < pair[1]),
        "ids reached the server out of order: {ids:?}"
    );
}

#[tokio::test]
async fn bad_token_is_an_auth_error() {
    let server = FakeServer::spawn(Arc::new(|_, _, _| support::Reply::ok(Value::Null))).await;
    let config = Config::direct(&server.base_url(), "wrong-token").expect("config");
    let Err(error) = Connection::connect(&config).await else {
        panic!("auth must fail");
    };
    assert!(matches!(error, HaError::Auth(_)));
    assert!(!format!("{error:?}").contains("wrong-token"));
}

#[tokio::test]
async fn request_timeout_is_reported() {
    let server = FakeServer::spawn(Arc::new(|_, _, _| support::Reply::silent())).await;
    let config = Config::direct(&server.base_url(), TOKEN)
        .expect("config")
        .with_request_timeout(Duration::from_millis(50));
    let connection = Connection::connect(&config).await.expect("connect");
    let error = connection
        .call_value("test/slow", Value::Null)
        .await
        .expect_err("must time out");
    assert!(matches!(error, HaError::Timeout(_)));
}

#[tokio::test]
async fn supervisor_reconnects_and_serves_the_next_call() {
    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = Arc::clone(&calls);
    let server = FakeServer::spawn(Arc::new(move |_, _, _| {
        let call = handler_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call == 1 {
            support::Reply::ok(json!({"call": 1})).close_after()
        } else {
            support::Reply::ok(json!({"call": call}))
        }
    }))
    .await;
    let config = Config::direct(&server.base_url(), TOKEN)
        .expect("config")
        .with_reconnect(ReconnectPolicy {
            initial: Duration::from_millis(10),
            max: Duration::from_millis(20),
            jitter: false,
        });
    let client = HaClient::new(config).expect("client");

    let first: Value = client
        .call("test/echo", Value::Null)
        .await
        .expect("first call");
    assert_eq!(first["call"], 1);

    tokio::time::timeout(Duration::from_secs(2), async {
        while server.connections.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("second connection");

    let second: Value = client
        .call("test/echo", Value::Null)
        .await
        .expect("second call");
    assert_eq!(second["call"], 2);
    assert!(server.requests() >= 2);
}
