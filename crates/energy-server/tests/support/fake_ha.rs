//! An in-process Home Assistant: the REST states endpoint plus the WebSocket commands discovery,
//! statistics and the solar forecast need.
//!
//! Deliberately hermetic: everything is generated from a clock and two constants, so the boot test
//! exercises the real `energy-ha` client, the real plan source and the real price fallback without a
//! network. The auth handshake is the real one; the token is the test's.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::Path;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use energy_price::book::query::{local_date_of, local_midnight};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// The token the fake accepts.
pub const TOKEN: &str = "test-token-not-a-secret";

const GRID_POWER: &str = "sensor.grid_power";
const GRID_IMPORT: &str = "sensor.grid_import";
const GRID_EXPORT: &str = "sensor.grid_export";
const SOLAR_POWER: &str = "sensor.solar_power";
const SOLAR_ENERGY: &str = "sensor.solar_energy";
const BATTERY_POWER: &str = "sensor.battery_power";
const BATTERY_CHARGE: &str = "sensor.battery_charge";
const BATTERY_DISCHARGE: &str = "sensor.battery_discharge";
const BATTERY_SOC: &str = "sensor.battery_soc";
const PRICE: &str = "sensor.zonneplan_current_quarter_hourly_electricity_tariff";

/// A running fake Home Assistant.
pub struct FakeHa {
    addr: SocketAddr,
    task: JoinHandle<()>,
}

impl FakeHa {
    /// Bind `127.0.0.1:0` and serve both transports.
    pub async fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the fake HA");
        let addr = listener.local_addr().expect("fake HA address");
        let router = router();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Self { addr, task }
    }

    /// The HA root URL (no `/api` suffix), as `Config::direct` expects.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for FakeHa {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn router() -> Router {
    Router::new()
        .route("/api/config", get(core_config))
        .route("/api/states", get(states))
        .route("/api/states/{entity}", get(state))
        .route("/api/websocket", get(socket))
        .with_state(Arc::new(()))
}

/// `GET /api/config`: a plausible Core, enough for the doctor's transport row.
async fn core_config() -> Json<Value> {
    Json(json!({
        "version": "2026.10.0",
        "time_zone": "Europe/Amsterdam",
        "currency": "EUR",
        "country": "NL",
        "language": "nl",
        "state": "RUNNING",
    }))
}

async fn states() -> Json<Value> {
    Json(Value::Array(vec![
        entity(GRID_POWER, "-450", "W"),
        entity(SOLAR_POWER, "1200", "W"),
        entity(BATTERY_POWER, "0", "W"),
        entity(BATTERY_SOC, "55", "%"),
    ]))
}

async fn state(Path(id): Path<String>) -> Json<Value> {
    if id == PRICE {
        return Json(price_sensor());
    }
    let watts = match id.as_str() {
        GRID_POWER => "-450",
        SOLAR_POWER => "1200",
        _ => "0",
    };
    Json(state_json(&id, watts, "W"))
}

fn entity(id: &str, state: &str, unit: &str) -> Value {
    state_json(id, state, unit)
}

fn state_json(id: &str, state: &str, unit: &str) -> Value {
    let now = stamp(Utc::now().timestamp());
    json!({
        "entity_id": id,
        "state": state,
        "attributes": {"unit_of_measurement": unit},
        "last_changed": now,
        "last_updated": now,
    })
}

/// A day and a half of quarter-hour prices from local midnight, in the sensor's forecast shape.
fn price_sensor() -> Value {
    let now = Utc::now().timestamp();
    let today = local_date_of(now).expect("a local date");
    let start = local_midnight(today).expect("local midnight");
    let mut entries = Vec::new();
    for index in 0..=192 {
        let t_start = start + i64::from(index) * 900;
        let t_end = t_start + 900;
        let amount = 0.20 + f64::from(index % 8) * 0.01;
        entries.push(json!({
            "start_date": stamp(t_start),
            "end_date": stamp(t_end),
            "price_tax_included": {"amount": amount},
        }));
    }
    json!({
        "entity_id": PRICE,
        "state": "0.20",
        "attributes": {"unit_of_measurement": "€/kWh", "forecast": entries},
        "last_changed": "2026-01-01T00:00:00+00:00",
        "last_updated": "2026-01-01T00:00:00+00:00",
    })
}

fn stamp(epoch: i64) -> String {
    DateTime::<Utc>::from_timestamp(epoch, 0)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339()
}

async fn socket(upgrade: WebSocketUpgrade) -> impl IntoResponse {
    upgrade.on_upgrade(serve_socket)
}

async fn serve_socket(mut socket: WebSocket) {
    let hello = json!({"type": "auth_required", "ha_version": "2026.10.0"});
    if socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    let Some(Ok(Message::Text(auth))) = socket.next().await else {
        return;
    };
    let token = serde_json::from_str::<Value>(&auth).ok().and_then(|value| {
        value
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    if token.as_deref() != Some(TOKEN) {
        let failure = json!({"type": "auth_invalid", "message": "bad token"});
        let _ = socket.send(Message::Text(failure.to_string().into())).await;
        return;
    }
    let ok = json!({"type": "auth_ok", "ha_version": "2026.10.0"});
    if socket
        .send(Message::Text(ok.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    while let Some(Ok(Message::Text(text))) = socket.next().await {
        let Ok(request) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let reply = answer(&request);
        if socket
            .send(Message::Text(reply.to_string().into()))
            .await
            .is_err()
        {
            return;
        }
    }
}

/// One request → one `result` (or `error`) frame.
fn answer(request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    match request.get("type").and_then(Value::as_str) {
        Some("energy/get_prefs") => {
            json!({"id": id, "type": "result", "success": true, "result": prefs()})
        }
        Some("energy/solar_forecast") => {
            json!({"id": id, "type": "result", "success": true, "result": solar_forecast()})
        }
        Some("recorder/statistics_during_period") => {
            json!({"id": id, "type": "result", "success": true, "result": statistics(request)})
        }
        Some("recorder/get_statistics_metadata") => {
            json!({"id": id, "type": "result", "success": true, "result": metadata(request)})
        }
        _ => json!({"id": id, "type": "result", "success": false,
                    "error": {"code": "not_found", "message": "unknown command"}}),
    }
}

fn prefs() -> Value {
    json!({
        "energy_sources": [
            {"type": "grid", "stat_energy_from": GRID_IMPORT, "stat_energy_to": GRID_EXPORT,
             "stat_rate": GRID_POWER, "name": "Grid"},
            {"type": "solar", "stat_energy_from": SOLAR_ENERGY, "stat_rate": SOLAR_POWER,
             "config_entry_solar_forecast": ["entry"], "name": "Roof"},
            {"type": "battery", "stat_energy_from": BATTERY_DISCHARGE, "stat_energy_to": BATTERY_CHARGE,
             "stat_soc": BATTERY_SOC, "stat_rate": BATTERY_POWER, "capacity": 7.68, "name": "Battery"}
        ],
        "device_consumption": [],
    })
}

fn solar_forecast() -> Value {
    let now = Utc::now().timestamp();
    let start = now - now.rem_euclid(900);
    let mut wh_hours = BTreeMap::new();
    for index in 0..=121 {
        let moment = start + i64::from(index) * 900;
        let hour = DateTime::<Utc>::from_timestamp(moment, 0)
            .map(|stamp| stamp.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .unwrap_or_default();
        let watts = if (7..19).contains(&local_hour(moment)) {
            2000.0
        } else {
            0.0
        };
        wh_hours.insert(hour, watts * 0.25);
    }
    json!({"entry": {"wh_hours": wh_hours}})
}

/// Statistics rows for the requested period; the first row is the warm-up row the parser drops.
fn statistics(request: &Value) -> Value {
    let period = request
        .get("period")
        .and_then(Value::as_str)
        .unwrap_or("5minute");
    let step = if period == "hour" { 3_600_000 } else { 300_000 };
    let start = request
        .get("start_time")
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map_or(0, |stamp| stamp.timestamp_millis());
    let end = request
        .get("end_time")
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map_or(start, |stamp| stamp.timestamp_millis());
    let ids = request
        .get("statistic_ids")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut result = serde_json::Map::new();
    for id in ids {
        let Some(entity) = id.as_str() else {
            continue;
        };
        let mut rows = Vec::new();
        let mut at = start.div_euclid(step) * step;
        while at <= end {
            rows.push(json!({
                "start": at,
                "end": at + step,
                "change": change(entity, at, step),
                "mean": if entity.contains("soc") { 55.0 } else { 0.0 },
            }));
            at += step;
        }
        result.insert(entity.to_owned(), Value::Array(rows));
    }
    Value::Object(result)
}

/// `recorder/get_statistics_metadata`: the units the doctor's stats rows read.
fn metadata(request: &Value) -> Value {
    let ids = request
        .get("statistic_ids")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        ids.iter()
            .filter_map(Value::as_str)
            .map(|id| {
                let (unit, class) = if id.contains("soc") {
                    ("%", "battery")
                } else {
                    ("kWh", "energy")
                };
                json!({
                    "statistic_id": id,
                    "display_unit_of_measurement": unit,
                    "statistics_unit_of_measurement": unit,
                    "unit_class": class,
                    "has_sum": !id.contains("soc"),
                    "has_mean": id.contains("soc"),
                })
            })
            .collect(),
    )
}

/// A synthetic, always-positive counter change in kWh for one slot.
fn change(entity: &str, start_ms: i64, step_ms: i64) -> f64 {
    let hours = step_ms as f64 / 3_600_000.0;
    let instant = start_ms.div_euclid(1000);
    let day = (7..19).contains(&local_hour(instant));
    if entity.contains("import") {
        0.20 * hours
    } else if entity.contains("export") {
        0.01 * hours
    } else if entity.contains("solar") {
        if day { 0.40 * hours } else { 0.0 }
    } else if entity.contains("discharge") || entity.contains("charge") {
        0.0
    } else {
        0.10 * hours
    }
}

/// The local (Europe/Amsterdam) hour of an instant, without pulling a timezone crate into the test.
fn local_hour(epoch: i64) -> i64 {
    // The test only needs a plausible day/night split; CET (+1/+2) is close enough for 7..19.
    (epoch.div_euclid(3600).rem_euclid(24) + 1).rem_euclid(24)
}

/// The default price sensor id (so a test can seed the same one).
pub const PRICE_ENTITY: &str = PRICE;
