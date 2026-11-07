//! SSE smoke tests: the first event, the heartbeat and a plan change.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, header};
use futures_util::StreamExt;
use tower::ServiceExt;

use support::{Fixture, SLOT};

async fn open_stream(
    fixture: &Fixture,
) -> (
    header::HeaderValue,
    futures_util::stream::BoxStream<'static, Vec<u8>>,
) {
    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/stream")
        .body(Body::empty())
        .expect("a request");
    let response = fixture
        .router()
        .oneshot(request)
        .await
        .expect("the stream opens");
    assert_eq!(response.status(), 200);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .expect("a content type");
    let frames = response
        .into_body()
        .into_data_stream()
        .filter_map(|frame| async move { frame.ok().map(|bytes| bytes.to_vec()) });
    (content_type, Box::pin(frames))
}

async fn next_frame(frames: &mut futures_util::stream::BoxStream<'static, Vec<u8>>) -> String {
    let frame = tokio::time::timeout(Duration::from_secs(2), frames.next())
        .await
        .expect("a frame arrives before the timeout")
        .expect("the stream is still open");
    String::from_utf8(frame).expect("utf8")
}

#[tokio::test]
async fn the_stream_starts_with_a_state_snapshot() {
    let fixture = Fixture::new();
    let (content_type, mut frames) = open_stream(&fixture).await;
    assert_eq!(content_type.to_str().unwrap(), "text/event-stream");
    let first = next_frame(&mut frames).await;
    assert!(first.contains("event: state"), "got {first:?}");
    assert!(first.contains("\"mode\":\"dry_run\""));
    assert!(first.contains("\"sn\":\"AB2000-ONE\""));
    assert!(first.contains("\"blocked\":true"));
    assert!(first.contains("\"kind\":\"stale_forecast\""));
}

#[tokio::test]
async fn a_quiet_stream_gets_heartbeats() {
    let fixture = Fixture::with_heartbeat(Duration::from_millis(40));
    let (_, mut frames) = open_stream(&fixture).await;
    let first = next_frame(&mut frames).await;
    assert!(first.contains("event: state"));
    let second = next_frame(&mut frames).await;
    assert!(second.contains("event: heartbeat"), "got {second:?}");
    assert!(second.contains("\"at\":\""));
}

#[tokio::test]
async fn a_new_plan_emits_a_plan_event_and_a_state_update() {
    let fixture = Fixture::new();
    let (_, mut frames) = open_stream(&fixture).await;
    let first = next_frame(&mut frames).await;
    assert!(first.contains("event: state"));

    let mut snapshot = fixture.service.snapshot();
    let plan = snapshot.plan.as_mut().expect("a plan in the snapshot");
    plan.plan_id = 4_242;
    plan.generated_at += SLOT;
    fixture.handles.publish(snapshot);

    let second = next_frame(&mut frames).await;
    assert!(second.contains("event: state"));
    assert!(second.contains("4242"));
    let third = next_frame(&mut frames).await;
    assert!(third.contains("event: plan"), "got {third:?}");
    assert!(third.contains("\"planId\":4242"));
}
