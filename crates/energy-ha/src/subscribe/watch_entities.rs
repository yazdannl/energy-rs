//! `subscribe_entities` with a `subscribe_events` fallback.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tracing::debug;

use crate::{
    client::{Connection, EventStream, HaError},
    state::EntityState,
};

use super::{EntityDiffEvent, EntityStore, EntityUpdate, EntityWatch};

/// Subscribe to a set of entities.
///
/// Tries `subscribe_entities` first and reads its initial compressed snapshot; falls back to
/// `subscribe_events`/`state_changed`. `seed` is the REST startup snapshot used by the fallback
/// (pass an empty vector if unavailable).
pub async fn watch_entities(
    connection: &Connection,
    entity_ids: &[String],
    seed: Vec<EntityState>,
) -> Result<EntityWatch, HaError> {
    if entity_ids.is_empty() {
        let (_sender, receiver) = mpsc::channel(1);
        return Ok(EntityWatch::new(EntityStore::from_states(seed), receiver));
    }

    match connection
        .subscribe("subscribe_entities", json!({ "entity_ids": entity_ids }))
        .await
    {
        Ok(events) => watch_compressed(connection, events).await,
        Err(error) => {
            debug!(error = %error, "subscribe_entities unavailable; using subscribe_events");
            watch_state_changed(connection, entity_ids, seed).await
        }
    }
}

async fn watch_compressed(
    connection: &Connection,
    mut events: EventStream,
) -> Result<EntityWatch, HaError> {
    let first = events.recv_timeout(connection.request_timeout()).await?;
    let mut store = EntityStore::default();
    let mut pending = Vec::new();
    if let Some(value) = first {
        match serde_json::from_value::<EntityDiffEvent>(value) {
            Ok(event) => {
                pending = store.apply(&event);
                // The initial `a` rows are already in `snapshot`; only send later diffs.
                pending.retain(|update| !matches!(update, EntityUpdate::Added(_)));
            }
            Err(error) => debug!(error = %error, "invalid subscribe_entities snapshot"),
        }
    }
    let snapshot = store.clone();
    let (sender, receiver) = mpsc::channel(256);
    tokio::spawn(async move {
        for update in pending {
            if sender.send(update).await.is_err() {
                return;
            }
        }
        while let Some(value) = events.recv().await {
            match serde_json::from_value::<EntityDiffEvent>(value) {
                Ok(event) => {
                    for update in store.apply(&event) {
                        if sender.send(update).await.is_err() {
                            return;
                        }
                    }
                }
                Err(error) => debug!(error = %error, "invalid subscribe_entities event"),
            }
        }
    });
    Ok(EntityWatch::new(snapshot, receiver))
}

async fn watch_state_changed(
    connection: &Connection,
    entity_ids: &[String],
    seed: Vec<EntityState>,
) -> Result<EntityWatch, HaError> {
    let mut events = connection
        .subscribe("subscribe_events", json!({ "event_type": "state_changed" }))
        .await?;
    let wanted: BTreeSet<String> = entity_ids.iter().cloned().collect();
    let snapshot = EntityStore::from_states(seed);
    let (sender, receiver) = mpsc::channel(256);
    tokio::spawn(async move {
        while let Some(value) = events.recv().await {
            let Some(update) = state_changed_update(value, &wanted) else {
                continue;
            };
            if sender.send(update).await.is_err() {
                return;
            }
        }
    });
    Ok(EntityWatch::new(snapshot, receiver))
}

#[derive(Debug, Clone, Default, Deserialize)]
struct StateChangedEnvelope {
    #[serde(default)]
    event: StateChangedEvent,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct StateChangedEvent {
    #[serde(default)]
    data: StateChangedData,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct StateChangedData {
    #[serde(default)]
    entity_id: String,
    #[serde(default)]
    old_state: Option<EntityState>,
    #[serde(default)]
    new_state: Option<EntityState>,
}

fn state_changed_update(value: Value, wanted: &BTreeSet<String>) -> Option<EntityUpdate> {
    let envelope: StateChangedEnvelope = serde_json::from_value(value).ok()?;
    let data = envelope.event.data;
    let entity_id = if data.entity_id.is_empty() {
        data.new_state.as_ref()?.entity_id.clone()
    } else {
        data.entity_id
    };
    if !wanted.contains(&entity_id) {
        return None;
    }
    match data.new_state {
        Some(state) => Some(if data.old_state.is_some() {
            EntityUpdate::Changed(state)
        } else {
            EntityUpdate::Added(state)
        }),
        None => Some(EntityUpdate::Removed(entity_id)),
    }
}
