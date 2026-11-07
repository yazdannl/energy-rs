#![allow(dead_code)]

use std::{
    error::Error,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_tungstenite::{WebSocketStream, accept_async, tungstenite::Message};

pub const TOKEN: &str = "test-token";

pub type Handler = Arc<dyn Fn(u64, &str, &Value) -> Reply + Send + Sync>;

#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub result: Value,
    pub error: Option<(String, String)>,
    pub events: Vec<Value>,
    pub close_after: bool,
    pub silent: bool,
}

impl Reply {
    pub fn ok(result: Value) -> Self {
        Self {
            result,
            ..Self::default()
        }
    }

    pub fn fail(code: &str, message: &str) -> Self {
        Self {
            error: Some((code.to_owned(), message.to_owned())),
            ..Self::default()
        }
    }

    pub fn with_event(mut self, event: Value) -> Self {
        self.events.push(event);
        self
    }

    pub fn close_after(mut self) -> Self {
        self.close_after = true;
        self
    }

    pub fn silent() -> Self {
        Self {
            silent: true,
            ..Self::default()
        }
    }
}

pub struct FakeServer {
    addr: SocketAddr,
    pub connections: Arc<AtomicUsize>,
    pub request_ids: Arc<Mutex<Vec<u64>>>,
    pub request_types: Arc<Mutex<Vec<String>>>,
    task: JoinHandle<()>,
}

impl FakeServer {
    pub async fn spawn(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake server");
        let addr = listener.local_addr().expect("fake server address");
        let connections = Arc::new(AtomicUsize::new(0));
        let request_ids = Arc::new(Mutex::new(Vec::new()));
        let request_types = Arc::new(Mutex::new(Vec::new()));

        let accept_connections = Arc::clone(&connections);
        let accept_ids = Arc::clone(&request_ids);
        let accept_types = Arc::clone(&request_types);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _peer)) = listener.accept().await else {
                    break;
                };
                accept_connections.fetch_add(1, Ordering::SeqCst);
                let handler = Arc::clone(&handler);
                let ids = Arc::clone(&accept_ids);
                let types = Arc::clone(&accept_types);
                tokio::spawn(async move {
                    let _ = serve(stream, handler, ids, types).await;
                });
            }
        });

        Self {
            addr,
            connections,
            request_ids,
            request_types,
            task,
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn requests(&self) -> usize {
        self.request_ids
            .lock()
            .map(|ids| ids.len())
            .unwrap_or_default()
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(
    stream: tokio::net::TcpStream,
    handler: Handler,
    request_ids: Arc<Mutex<Vec<u64>>>,
    request_types: Arc<Mutex<Vec<String>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut socket = accept_async(stream).await?;
    send_json(
        &mut socket,
        json!({"type": "auth_required", "ha_version": "2026.10.4"}),
    )
    .await?;

    let auth = next_json(&mut socket).await?.ok_or("closed before auth")?;
    let token_ok = auth.get("access_token").and_then(Value::as_str) == Some(TOKEN);
    let type_ok = auth.get("type").and_then(Value::as_str) == Some("auth");
    if !token_ok || !type_ok {
        send_json(
            &mut socket,
            json!({"type": "auth_invalid", "message": "Invalid access token"}),
        )
        .await?;
        return Ok(());
    }
    send_json(
        &mut socket,
        json!({"type": "auth_ok", "ha_version": "2026.10.4"}),
    )
    .await?;

    loop {
        let Some(request) = next_json(&mut socket).await? else {
            break;
        };
        let Some(id) = request.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let command_type = request
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if command_type == "unsubscribe_events" {
            continue;
        }
        request_ids.lock().expect("request ids").push(id);
        request_types
            .lock()
            .expect("request types")
            .push(command_type.clone());

        let reply = handler(id, &command_type, &request);
        if reply.silent {
            continue;
        }
        if let Some((code, message)) = reply.error {
            send_json(
                &mut socket,
                json!({
                    "id": id,
                    "type": "result",
                    "success": false,
                    "error": {"code": code, "message": message},
                }),
            )
            .await?;
        } else {
            send_json(
                &mut socket,
                json!({
                    "id": id,
                    "type": "result",
                    "success": true,
                    "result": reply.result,
                }),
            )
            .await?;
        }
        for event in reply.events {
            send_json(
                &mut socket,
                json!({"id": id, "type": "event", "event": event}),
            )
            .await?;
        }
        if reply.close_after {
            let _ = socket.close(None).await;
            break;
        }
    }
    Ok(())
}

async fn send_json(
    socket: &mut WebSocketStream<tokio::net::TcpStream>,
    value: Value,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    socket
        .send(Message::Text(serde_json::to_string(&value)?.into()))
        .await?;
    Ok(())
}

async fn next_json(
    socket: &mut WebSocketStream<tokio::net::TcpStream>,
) -> Result<Option<Value>, Box<dyn Error + Send + Sync>> {
    loop {
        match socket.next().await {
            Some(Ok(Message::Text(text))) => {
                return Ok(Some(serde_json::from_str(text.as_str())?));
            }
            Some(Ok(Message::Ping(payload))) => {
                socket.send(Message::Pong(payload)).await?;
            }
            Some(Ok(Message::Close(_))) | None => return Ok(None),
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(Box::new(error)),
        }
    }
}

/// Load a scrubbed JSON fixture from `tests/fixtures`.
pub fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    let text = std::fs::read_to_string(&path).expect("read fixture");
    serde_json::from_str(&text).expect("parse fixture")
}

pub fn result(name: &str) -> Value {
    fixture(name)
        .get("result")
        .cloned()
        .expect("fixture has result")
}
