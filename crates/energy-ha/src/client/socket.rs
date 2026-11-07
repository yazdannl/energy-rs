//! WebSocket auth handshake and reader/writer loop.

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::Arc;

use tokio::{net::TcpStream, sync::mpsc, time::timeout};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tracing::debug;

use super::{
    Config, HaError,
    connection::{Outgoing, Shared},
};

pub(crate) type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(crate) async fn handshake(config: &Config) -> Result<(Socket, String), HaError> {
    let (mut socket, _response) = timeout(config.connect_timeout, connect_async(config.ws_url()))
        .await
        .map_err(|_| HaError::Timeout(config.connect_timeout))?
        .map_err(|error| HaError::Ws(config.redact(&error.to_string())))?;

    let first = read_auth_frame(&mut socket, config).await?;
    match first.get("type").and_then(Value::as_str) {
        Some("auth_ok") => Ok((socket, version_of(&first))),
        Some("auth_required") => {
            let fallback_version = version_of(&first);
            send_auth(&mut socket, config).await?;
            let reply = read_auth_frame(&mut socket, config).await?;
            authenticate(socket, &reply, fallback_version, config)
        }
        other => Err(HaError::Protocol(format!(
            "unexpected first WebSocket frame {other:?}"
        ))),
    }
}

fn authenticate(
    socket: Socket,
    reply: &Value,
    fallback_version: String,
    config: &Config,
) -> Result<(Socket, String), HaError> {
    match reply.get("type").and_then(Value::as_str) {
        Some("auth_ok") => {
            let version = reply
                .get("ha_version")
                .and_then(Value::as_str)
                .map_or(fallback_version, str::to_owned);
            Ok((socket, version))
        }
        Some("auth_invalid") => {
            let message = reply
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("invalid access token");
            Err(HaError::Auth(config.redact(message)))
        }
        other => Err(HaError::Protocol(format!(
            "unexpected auth reply {other:?}"
        ))),
    }
}

fn version_of(frame: &Value) -> String {
    frame
        .get("ha_version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

async fn read_auth_frame(socket: &mut Socket, config: &Config) -> Result<Value, HaError> {
    loop {
        let message = timeout(config.connect_timeout, socket.next())
            .await
            .map_err(|_| HaError::Timeout(config.connect_timeout))?
            .ok_or(HaError::Disconnected)?;

        match message {
            Ok(Message::Text(text)) => {
                return serde_json::from_str(text.as_str()).map_err(|error| {
                    HaError::Protocol(format!("invalid auth frame from Home Assistant: {error}"))
                });
            }
            Ok(Message::Ping(payload)) => {
                let _ = timeout(config.connect_timeout, socket.send(Message::Pong(payload))).await;
            }
            Ok(Message::Pong(_) | Message::Binary(_) | Message::Frame(_)) => {}
            Ok(Message::Close(_)) => {
                return Err(HaError::Ws(
                    "connection closed during authentication".to_owned(),
                ));
            }
            Err(error) => return Err(HaError::Ws(config.redact(&error.to_string()))),
        }
    }
}

async fn send_auth(socket: &mut Socket, config: &Config) -> Result<(), HaError> {
    let auth = json!({
        "type": "auth",
        "access_token": config.token(),
    });
    let text = serde_json::to_string(&auth)?;
    timeout(
        config.connect_timeout,
        socket.send(Message::Text(text.into())),
    )
    .await
    .map_err(|_| HaError::Timeout(config.connect_timeout))?
    .map_err(|error| HaError::Ws(config.redact(&error.to_string())))?;
    Ok(())
}

/// Reader/writer loop. Stops on socket close, transport error, or an explicit close.
pub(crate) async fn run(
    mut socket: Socket,
    mut out_rx: mpsc::Receiver<Outgoing>,
    shared: Arc<Shared>,
) {
    let close_error = loop {
        let outcome = tokio::select! {
            biased;
            incoming = socket.next() => on_frame(&mut socket, &shared, incoming).await,
            outgoing = out_rx.recv() => on_outgoing(&mut socket, outgoing).await,
        };
        match outcome {
            Ok(true) => {}
            Ok(false) => break HaError::Disconnected,
            Err(error) => break error,
        }
    };
    shared.fail_all(&close_error);
    shared.mark_closed();
}

/// Handle one incoming frame; `Ok(false)` stops the loop.
async fn on_frame(
    socket: &mut Socket,
    shared: &Shared,
    incoming: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
) -> Result<bool, HaError> {
    match incoming {
        Some(Ok(Message::Text(text))) => {
            log_invalid_frame(shared, text.as_str());
            Ok(true)
        }
        Some(Ok(Message::Binary(_))) => {
            debug!("ignoring binary Home Assistant frame");
            Ok(true)
        }
        Some(Ok(Message::Ping(payload))) => answer_ping(socket, payload).await,
        Some(Ok(Message::Pong(_) | Message::Frame(_))) => Ok(true),
        Some(Ok(Message::Close(_))) | None => Ok(false),
        Some(Err(error)) => Err(HaError::Ws(error.to_string())),
    }
}

/// Parse one text frame; an invalid frame is logged and dropped, never fatal.
fn log_invalid_frame(shared: &Shared, text: &str) {
    if let Err(error) = handle_frame(shared, text) {
        debug!(error = %error, "ignoring invalid Home Assistant frame");
    }
}

/// Answer a ping; a failed send is a transport error.
async fn answer_ping(
    socket: &mut Socket,
    payload: tokio_tungstenite::tungstenite::Bytes,
) -> Result<bool, HaError> {
    socket
        .send(Message::Pong(payload))
        .await
        .map_err(|error| HaError::Ws(error.to_string()))?;
    Ok(true)
}

/// Handle one queued outgoing message; `Ok(false)` stops the loop.
async fn on_outgoing(socket: &mut Socket, outgoing: Option<Outgoing>) -> Result<bool, HaError> {
    match outgoing {
        Some(Outgoing::Text(text)) => {
            socket
                .send(Message::Text(text.into()))
                .await
                .map_err(|error| HaError::Ws(error.to_string()))?;
            Ok(true)
        }
        Some(Outgoing::Close) => {
            let _ = socket.close(None).await;
            Ok(false)
        }
        None => Ok(false),
    }
}

fn handle_frame(shared: &Shared, text: &str) -> Result<(), HaError> {
    let value: Value = serde_json::from_str(text)?;
    match value {
        Value::Array(messages) => {
            for message in messages {
                handle_message(shared, &message)?;
            }
        }
        message => handle_message(shared, &message)?,
    }
    Ok(())
}

fn handle_message(shared: &Shared, message: &Value) -> Result<(), HaError> {
    let Some(object) = message.as_object() else {
        return Err(HaError::Protocol("frame is not a JSON object".to_owned()));
    };
    match object.get("type").and_then(Value::as_str) {
        Some("result") => handle_result(shared, object),
        Some("event") => handle_event(shared, object),
        Some("pong") => Ok(()),
        Some(other) => {
            debug!(
                frame_type = other,
                "ignoring unsolicited Home Assistant frame"
            );
            Ok(())
        }
        None => Err(HaError::Protocol("frame without type".to_owned())),
    }
}

fn handle_result(shared: &Shared, object: &serde_json::Map<String, Value>) -> Result<(), HaError> {
    let Some(id) = object.get("id").and_then(Value::as_u64) else {
        return Err(HaError::Protocol("result without id".to_owned()));
    };
    let Some(waiter) = shared.take_waiter(id) else {
        return Ok(());
    };
    let success = object
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let result = if success {
        Ok(object.get("result").cloned().unwrap_or(Value::Null))
    } else {
        let error = object.get("error");
        Err(HaError::Api {
            code: error
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned(),
            message: error
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("no message")
                .to_owned(),
        })
    };
    let _ = waiter.send(result);
    Ok(())
}

fn handle_event(shared: &Shared, object: &serde_json::Map<String, Value>) -> Result<(), HaError> {
    let Some(id) = object.get("id").and_then(Value::as_u64) else {
        return Err(HaError::Protocol("event without id".to_owned()));
    };
    let event = object.get("event").cloned().unwrap_or(Value::Null);
    if let Some(sender) = shared.event_sender(id) {
        // Bounded: a slow consumer drops live samples instead of stalling the reader.
        let _ = sender.try_send(event);
    }
    Ok(())
}
