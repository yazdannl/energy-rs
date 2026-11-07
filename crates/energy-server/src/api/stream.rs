//! `GET /api/v1/stream` — Server-Sent Events over the engine's snapshot channel.
//!
//! Event names (stable):
//!
//! * `state` — the full live snapshot: mode, last tick, plan id, pack states, meter, forecast and
//!   the open faults. Sent once on connect and again after every tick that changed the snapshot.
//! * `plan` — the plan id changed (a new plan was solved). The client then reads
//!   `/api/v1/plan/current`; the event carries only the id and the solve instant.
//! * `fault` — one fault opened (or re-opened with a new `since`). A fault that disappears is only
//!   visible through the next `state`.
//! * `heartbeat` — `{"at": ...}`; sent whenever the stream was quiet for the configured interval.
//!
//! Backpressure: the source is a `tokio::sync::watch` channel, so a slow client never grows a
//! queue. It is woken for the newest snapshot and reads that one; intermediate ticks are coalesced
//! (dropped by design, not buffered).

use std::collections::{HashSet, VecDeque};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use futures_util::stream::{self, Stream};
use serde::Serialize;
use serde_json::json;
use tokio::sync::watch;
use tokio::time::{Instant, sleep_until};

use crate::api::dto::Timestamp;
use crate::api::dto::units::watt_hours;
use crate::service::Service;

use energy_core::safety::Authority;
use energy_core::stream::EngineSnapshot;

/// One pack in a `state` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LivePack {
    sn: String,
    soc_pct: Option<f64>,
    power_w: Option<f64>,
    stale: bool,
    blocked: bool,
}

/// The meter in a `state` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveMeter {
    at: Option<Timestamp>,
    grid_w: f64,
    pv_w: f64,
    load_w: f64,
    battery_w: Option<f64>,
}

/// The forecast totals in a `state` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveForecast {
    at: Option<Timestamp>,
    pv_wh: f64,
    load_wh: f64,
    confidence: Option<f64>,
}

/// One fault, in a `state` or `fault` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveFault {
    kind: String,
    message: String,
    since: Option<Timestamp>,
}

/// A `plan` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanEvent {
    plan_id: Option<i64>,
    at: Timestamp,
}

/// A `state` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StateEvent {
    mode: Authority,
    last_tick_at: Option<Timestamp>,
    plan_id: Option<i64>,
    packs: Vec<LivePack>,
    meter: Option<LiveMeter>,
    forecast: Option<LiveForecast>,
    faults: Vec<LiveFault>,
}

impl StateEvent {
    fn from_snapshot(snapshot: &EngineSnapshot) -> Self {
        Self {
            mode: snapshot.mode,
            last_tick_at: Timestamp::lenient(snapshot.last_tick),
            plan_id: snapshot.plan.as_ref().map(|plan| plan.plan_id),
            packs: snapshot
                .packs
                .iter()
                .map(|pack| LivePack {
                    sn: pack.sn.clone(),
                    soc_pct: pack.soc_pct,
                    power_w: pack.power_w,
                    stale: pack.stale,
                    blocked: pack.latch.as_ref().is_some_and(|latch| latch.blocked),
                })
                .collect(),
            meter: snapshot.meter.as_ref().map(|meter| LiveMeter {
                at: Timestamp::lenient(Some(meter.at)),
                grid_w: meter.grid_w,
                pv_w: meter.pv_w,
                load_w: meter.load_w,
                battery_w: meter.battery_w,
            }),
            forecast: snapshot.forecast.as_ref().map(|forecast| LiveForecast {
                at: Timestamp::lenient(Some(forecast.at)),
                pv_wh: watt_hours(forecast.pv_wh),
                load_wh: watt_hours(forecast.load_wh),
                confidence: forecast.confidence,
            }),
            faults: snapshot.faults.iter().map(fault_event).collect(),
        }
    }
}

fn fault_event(fault: &energy_core::stream::FaultSnapshot) -> LiveFault {
    LiveFault {
        kind: fault.kind.clone(),
        message: fault.message.clone(),
        since: Timestamp::lenient(Some(fault.since)),
    }
}

fn encode(event: &str, data: Result<Event, axum::Error>) -> Event {
    data.unwrap_or_else(|_| Event::default().event(event).data("{}"))
}

/// The stream's per-connection state: channel, heartbeat clock and the already-sent plan/faults.
struct Cursor {
    receiver: watch::Receiver<EngineSnapshot>,
    heartbeat: Duration,
    next_heartbeat: Instant,
    pending: VecDeque<Event>,
    last_plan: Option<i64>,
    seen_faults: HashSet<(String, i64)>,
}

impl Cursor {
    fn new(
        receiver: watch::Receiver<EngineSnapshot>,
        heartbeat: Duration,
        initial: &EngineSnapshot,
    ) -> Self {
        let mut cursor = Self {
            receiver,
            heartbeat,
            next_heartbeat: Instant::now() + heartbeat,
            pending: VecDeque::new(),
            last_plan: initial.plan.as_ref().map(|plan| plan.plan_id),
            seen_faults: initial
                .faults
                .iter()
                .map(|fault| (fault.kind.clone(), fault.since))
                .collect(),
        };
        cursor.pending.push_back(encode(
            "state",
            Event::default()
                .event("state")
                .json_data(StateEvent::from_snapshot(initial)),
        ));
        cursor
    }

    async fn next_event(&mut self) -> Option<Event> {
        if let Some(event) = self.pending.pop_front() {
            return Some(event);
        }
        tokio::select! {
            changed = self.receiver.changed() => {
                if changed.is_err() {
                    return None;
                }
                let snapshot = self.receiver.borrow_and_update().clone();
                self.on_snapshot(&snapshot);
                self.pending.pop_front()
            }
            () = sleep_until(self.next_heartbeat) => {
                self.next_heartbeat = Instant::now() + self.heartbeat;
                Some(encode("heartbeat", Event::default().event("heartbeat").json_data(json!({
                    "at": Timestamp::now(),
                }))))
            }
        }
    }

    fn on_snapshot(&mut self, snapshot: &EngineSnapshot) {
        self.pending.push_back(encode(
            "state",
            Event::default()
                .event("state")
                .json_data(StateEvent::from_snapshot(snapshot)),
        ));
        let plan_id = snapshot.plan.as_ref().map(|plan| plan.plan_id);
        if plan_id != self.last_plan {
            self.last_plan = plan_id;
            let at = snapshot
                .plan
                .as_ref()
                .and_then(|plan| Timestamp::lenient(Some(plan.generated_at)))
                .unwrap_or_else(Timestamp::now);
            self.pending.push_back(encode(
                "plan",
                Event::default()
                    .event("plan")
                    .json_data(PlanEvent { plan_id, at }),
            ));
        }
        for fault in &snapshot.faults {
            let key = (fault.kind.clone(), fault.since);
            if self.seen_faults.insert(key) {
                self.pending.push_back(encode(
                    "fault",
                    Event::default()
                        .event("fault")
                        .json_data(fault_event(fault)),
                ));
            }
        }
    }
}

/// `GET /api/v1/stream`
pub(crate) async fn stream(
    State(service): State<Arc<Service>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let receiver = service.subscribe();
    let initial = receiver.borrow().clone();
    let cursor = Cursor::new(receiver, service.heartbeat(), &initial);
    let events = stream::unfold(cursor, |mut cursor| async move {
        cursor
            .next_event()
            .await
            .map(|event| (Ok::<Event, Infallible>(event), cursor))
    });
    Sse::new(events)
}
