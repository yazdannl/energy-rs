//! The loopback HTTP server for the fake device.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::error::BatteryError;
use crate::fake::state::FakeState;
use crate::transport::Endpoint;

/// A fake device on `127.0.0.1:0`.
///
/// The server answers `GET /properties/report` and `POST /properties/write` exactly like the device
/// does, so a test can build a real [`HttpTransport`](crate::HttpTransport) against it and exercise
/// the wire format, the pacing and the error mapping.
pub struct FakeDevice {
    addr: SocketAddr,
    state: Arc<Mutex<FakeState>>,
    task: Option<JoinHandle<()>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl FakeDevice {
    /// Bind an ephemeral loopback port and start serving.
    pub async fn spawn(state: FakeState) -> Result<Self, BatteryError> {
        let listener =
            TcpListener::bind("127.0.0.1:0")
                .await
                .map_err(|err| BatteryError::Invalid {
                    reason: format!("fake device cannot bind: {err}"),
                })?;
        let addr = listener.local_addr().map_err(|err| BatteryError::Invalid {
            reason: format!("fake device has no address: {err}"),
        })?;
        let state = Arc::new(Mutex::new(state));
        let router = router(Arc::clone(&state));
        let (shutdown, stop) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let served = axum::serve(listener, router).with_graceful_shutdown(async {
                let _ = stop.await;
            });
            let _ = served.await;
        });
        Ok(Self {
            addr,
            state,
            task: Some(task),
            shutdown: Some(shutdown),
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn host(&self) -> String {
        self.addr.ip().to_string()
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Endpoint for a client; `sn` defaults to the fake device's own serial.
    pub fn endpoint(&self, sn: &str) -> Endpoint {
        let sn = if sn.is_empty() {
            self.serial()
        } else {
            sn.to_owned()
        };
        Endpoint::with_port(self.host(), self.port(), sn)
    }

    /// A copy of the current device state.
    pub fn state(&self) -> Result<FakeState, BatteryError> {
        self.with_state(|state| state.clone())
    }

    pub fn serial(&self) -> String {
        self.state
            .lock()
            .map(|state| state.sn.clone())
            .unwrap_or_default()
    }

    /// Mutate the device (set limits, wedge it, change its behaviour) from a test.
    pub fn with_state<R>(
        &self,
        apply: impl FnOnce(&mut FakeState) -> R,
    ) -> Result<R, BatteryError> {
        let mut state = self.state.lock().map_err(|_| BatteryError::Invalid {
            reason: "fake device state is poisoned".to_owned(),
        })?;
        Ok(apply(&mut state))
    }

    /// Ask the server to stop and wait for it.
    pub async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for FakeDevice {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn router(state: Arc<Mutex<FakeState>>) -> Router {
    Router::new()
        .route("/properties/report", get(report_handler))
        .route("/properties/write", post(write_handler))
        .with_state(state)
}

async fn report_handler(State(state): State<Arc<Mutex<FakeState>>>) -> Response {
    let Ok(guard) = state.lock() else {
        return poisoned();
    };
    match guard.behaviour.http_status {
        Some(status) => status_response(
            status,
            json!({"code": status, "success": false, "sn": guard.sn, "error": "injected http status"}),
        ),
        None => (StatusCode::OK, Json(guard.report_json())).into_response(),
    }
}

async fn write_handler(State(state): State<Arc<Mutex<FakeState>>>, body: String) -> Response {
    let Ok(value) = serde_json::from_str::<Value>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"code": 400, "success": false, "error": "failed to parse json"})),
        )
            .into_response();
    };
    let Ok(mut guard) = state.lock() else {
        return poisoned();
    };
    let (status, reply) = guard.apply_write(&value);
    status_response(status, reply)
}

fn status_response(status: u16, body: Value) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(body)).into_response()
}

fn poisoned() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"code": 500, "success": false, "error": "state poisoned"})),
    )
        .into_response()
}
