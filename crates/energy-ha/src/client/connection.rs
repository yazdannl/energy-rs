//! Live authenticated WebSocket connection with request/event demultiplexing.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use tokio::{
    sync::{Notify, mpsc, oneshot},
    time::timeout,
};

use super::{Config, EventStream, HaError, socket};

pub(crate) const REQUEST_QUEUE: usize = 256;
pub(crate) const SUBSCRIPTION_QUEUE: usize = 1024;

pub(crate) enum Outgoing {
    Text(String),
    Close,
}

pub(crate) struct Shared {
    out: mpsc::Sender<Outgoing>,
    waiters: Mutex<HashMap<u64, oneshot::Sender<Result<Value, HaError>>>>,
    subscriptions: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    next_id: AtomicU64,
    closed: Notify,
    closed_flag: AtomicBool,
    request_timeout: Duration,
}

impl Shared {
    fn new(out: mpsc::Sender<Outgoing>, request_timeout: Duration) -> Self {
        Self {
            out,
            waiters: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            closed: Notify::new(),
            closed_flag: AtomicBool::new(false),
            request_timeout,
        }
    }

    pub(crate) fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    pub(crate) async fn send_value(&self, value: &Value) -> Result<(), HaError> {
        let text = serde_json::to_string(value)?;
        timeout(self.request_timeout, self.out.send(Outgoing::Text(text)))
            .await
            .map_err(|_| HaError::Timeout(self.request_timeout))?
            .map_err(|_| HaError::Disconnected)
    }

    pub(crate) fn register_waiter(&self, id: u64, sender: oneshot::Sender<Result<Value, HaError>>) {
        lock(&self.waiters).insert(id, sender);
    }

    pub(crate) fn take_waiter(&self, id: u64) -> Option<oneshot::Sender<Result<Value, HaError>>> {
        lock(&self.waiters).remove(&id)
    }

    pub(crate) fn remove_waiter(&self, id: u64) {
        lock(&self.waiters).remove(&id);
    }

    pub(crate) fn add_subscription(&self, id: u64, sender: mpsc::Sender<Value>) {
        lock(&self.subscriptions).insert(id, sender);
    }

    pub(crate) fn remove_subscription(&self, id: u64) {
        lock(&self.subscriptions).remove(&id);
    }

    pub(crate) fn event_sender(&self, id: u64) -> Option<mpsc::Sender<Value>> {
        lock(&self.subscriptions).get(&id).cloned()
    }

    pub(crate) fn fail_all(&self, error: HaError) {
        let waiters: Vec<_> = lock(&self.waiters)
            .drain()
            .map(|(_, sender)| sender)
            .collect();
        for waiter in waiters {
            let _ = waiter.send(Err(error.clone()));
        }
        lock(&self.subscriptions).clear();
    }

    pub(crate) fn mark_closed(&self) {
        self.closed_flag.store(true, Ordering::SeqCst);
        self.closed.notify_waiters();
    }

    fn is_closed(&self) -> bool {
        self.closed_flag.load(Ordering::SeqCst)
    }

    pub(crate) async fn wait_closed(&self) {
        let notified = self.closed.notified();
        if self.is_closed() {
            return;
        }
        notified.await;
    }

    pub(crate) fn try_send_text(&self, text: String) {
        let _ = self.out.try_send(Outgoing::Text(text));
    }

    pub(crate) fn try_close(&self) {
        let _ = self.out.try_send(Outgoing::Close);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// A live, authenticated WebSocket connection.
///
/// Requests use monotonically increasing integer ids; events reuse their subscription id. Bounded
/// writer and subscription queues mean a slow consumer loses events instead of stalling the reader.
pub struct Connection {
    shared: Arc<Shared>,
    ha_version: String,
}

impl Connection {
    /// Connect and authenticate once. The caller is responsible for reconnecting.
    pub async fn connect(config: &Config) -> Result<Self, HaError> {
        let (socket, ha_version) = socket::handshake(config).await?;
        let (out, out_rx) = mpsc::channel(REQUEST_QUEUE);
        let shared = Arc::new(Shared::new(out, config.request_timeout));
        let task_shared = Arc::clone(&shared);
        tokio::spawn(async move {
            socket::run(socket, out_rx, task_shared).await;
        });
        Ok(Self { shared, ha_version })
    }

    pub fn ha_version(&self) -> &str {
        &self.ha_version
    }

    pub fn request_timeout(&self) -> Duration {
        self.shared.request_timeout()
    }

    /// Resolves when the socket reader has stopped (server close, transport error, or close).
    pub async fn closed(&self) {
        self.shared.wait_closed().await;
    }

    /// Send one command and deserialize its `result`.
    pub async fn call<T: DeserializeOwned>(
        &self,
        command_type: &str,
        params: Value,
    ) -> Result<T, HaError> {
        let value = self.call_value(command_type, params).await?;
        serde_json::from_value(value).map_err(HaError::from)
    }

    /// Send one command and return the raw `result` value.
    pub async fn call_value(&self, command_type: &str, params: Value) -> Result<Value, HaError> {
        let id = self.shared.next_id();
        self.request_with_id(id, command_type, params).await
    }

    /// Subscribe to a command that streams `event` frames carrying the subscription id.
    pub async fn subscribe(
        &self,
        command_type: &str,
        params: Value,
    ) -> Result<EventStream, HaError> {
        let id = self.shared.next_id();
        let (sender, receiver) = mpsc::channel(SUBSCRIPTION_QUEUE);
        self.shared.add_subscription(id, sender);

        if let Err(error) = self.request_with_id(id, command_type, params).await {
            self.shared.remove_subscription(id);
            return Err(error);
        }
        Ok(EventStream::new(Arc::clone(&self.shared), id, receiver))
    }

    async fn request_with_id(
        &self,
        id: u64,
        command_type: &str,
        params: Value,
    ) -> Result<Value, HaError> {
        let (sender, receiver) = oneshot::channel();
        self.shared.register_waiter(id, sender);

        let request = request_object(id, command_type, params)?;
        if let Err(error) = self.shared.send_value(&request).await {
            self.shared.remove_waiter(id);
            return Err(error);
        }

        match timeout(self.shared.request_timeout(), receiver).await {
            Err(_) => {
                self.shared.remove_waiter(id);
                Err(HaError::Timeout(self.shared.request_timeout()))
            }
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(HaError::Disconnected),
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.shared.try_close();
    }
}

fn request_object(id: u64, command_type: &str, params: Value) -> Result<Value, HaError> {
    let mut object = match params {
        Value::Null => Map::new(),
        Value::Object(object) => object,
        _ => {
            return Err(HaError::Protocol(
                "command parameters must be a JSON object".to_owned(),
            ));
        }
    };
    object.insert("id".to_owned(), json!(id));
    object.insert("type".to_owned(), json!(command_type));
    Ok(Value::Object(object))
}
