//! Bounded I/O, cancellation cleanup, and uncertain hardware outcomes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::semicolon_if_nothing_returned
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::sync::Notify;

use energy_battery::control::{Command, ControlFault, ControlLimits, DeviceController};
use energy_battery::error::BatteryError;
use energy_battery::parse_report;
use energy_battery::report::Report;
use energy_battery::transport::{BoxFuture, DeviceIo, WriteAck, WriteRequest};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadMode {
    Ready,
    Pending,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WriteMode {
    Ready,
    Pending,
    AcceptedNoReply,
}

struct State(Mutex<InnerState>);

struct InnerState {
    read: ReadMode,
    write: WriteMode,
    message_id: u32,
    input_limit: i32,
    output_limit: i32,
}

struct Scripted {
    state: State,
    write_started: Arc<Notify>,
    writes: Arc<AtomicUsize>,
    holds: Arc<AtomicUsize>,
}

impl Scripted {
    fn new(write: WriteMode) -> Self {
        Self {
            state: State(Mutex::new(InnerState {
                read: ReadMode::Ready,
                write,
                message_id: 4211,
                input_limit: 0,
                output_limit: 0,
            })),
            write_started: Arc::new(Notify::new()),
            writes: Arc::new(AtomicUsize::new(0)),
            holds: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn set_read(&self, read: ReadMode) {
        self.state.0.lock().unwrap().read = read;
    }

    fn snapshot(&self) -> (u32, i32, i32) {
        let mut state = self.state.0.lock().unwrap();
        state.message_id += 1;
        (state.message_id, state.input_limit, state.output_limit)
    }
}

impl DeviceIo for Scripted {
    fn sn(&self) -> &'static str {
        "STUCK"
    }

    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
        let pending = self.state.0.lock().unwrap().read == ReadMode::Pending;
        let (message_id, input_limit, output_limit) =
            if pending { (0, 0, 0) } else { self.snapshot() };
        Box::pin(async move {
            if pending {
                std::future::pending().await
            }
            Ok(report(message_id, input_limit, output_limit))
        })
    }

    fn write(&self, request: WriteRequest) -> BoxFuture<'_, Result<WriteAck, BatteryError>> {
        let is_hold = request.properties().ac_mode().is_none();
        let writes = Arc::clone(&self.writes);
        let holds = Arc::clone(&self.holds);
        let started = Arc::clone(&self.write_started);
        let mode = {
            let mut state = self.state.0.lock().unwrap();
            if is_hold {
                state.input_limit = 0;
                state.output_limit = 0;
                holds.fetch_add(1, Ordering::Relaxed);
            } else {
                state.input_limit = 300;
                if state.write == WriteMode::AcceptedNoReply {
                    writes.fetch_add(1, Ordering::Relaxed);
                }
            }
            state.write
        };
        Box::pin(async move {
            if is_hold {
                return Ok(ack());
            }
            match mode {
                WriteMode::Ready => {
                    writes.fetch_add(1, Ordering::Relaxed);
                    Ok(ack())
                }
                WriteMode::AcceptedNoReply | WriteMode::Pending => {
                    started.notify_one();
                    std::future::pending().await
                }
            }
        })
    }
}

fn ack() -> WriteAck {
    WriteAck {
        timestamp: None,
        message_id: None,
        code: 200,
        sn: Some("STUCK".to_owned()),
    }
}

fn report(message_id: u32, input_limit: i32, output_limit: i32) -> Report {
    let document = json!({
        "timestamp": 1_791_959_470,
        "messageId": message_id,
        "sn": "STUCK",
        "version": 3,
        "product": "solarFlow800Plus",
        "properties": {
            "acMode": 2,
            "inputLimit": input_limit,
            "outputLimit": output_limit,
            "chargeMaxLimit": 1000,
            "inverseMaxLimit": 800
        },
        "packData": []
    });
    parse_report(&serde_json::to_vec(&document).expect("report json")).expect("parse")
}

fn controller(device: Arc<Scripted>) -> Arc<DeviceController<Arc<Scripted>>> {
    Arc::new(DeviceController::new(device, ControlLimits::unlimited()).with_spacing(Duration::ZERO))
}

#[tokio::test]
async fn a_hung_read_cannot_block_a_hold() {
    let device = Arc::new(Scripted::new(WriteMode::Ready));
    let control = controller(Arc::clone(&device));
    control.poll().await.expect("prime the report");
    device.set_read(ReadMode::Pending);

    let reader = Arc::clone(&control);
    let read = tokio::spawn(async move { reader.poll().await });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let holder = Arc::clone(&control);
    let held = tokio::time::timeout(Duration::from_secs(9), async move {
        holder.apply(Command::Hold).await
    })
    .await
    .expect("a hold must not wait behind a hung read");
    assert!(held.is_ok(), "hold failed: {:?}", held.err());
    assert_eq!(device.holds.load(Ordering::Relaxed), 1);
    let read = tokio::time::timeout(Duration::from_secs(2), read)
        .await
        .expect("read task must stop at its deadline");
    assert!(matches!(read, Ok(Err(_))), "the hung read must be bounded");
}

#[tokio::test]
async fn cancelling_a_write_clears_pending_so_the_next_command_is_accepted() {
    let device = Arc::new(Scripted::new(WriteMode::Pending));
    let started = Arc::clone(&device.write_started);
    let control = controller(Arc::clone(&device));
    control.poll().await.expect("prime the report");

    let writer = Arc::clone(&control);
    let write = tokio::spawn(async move { writer.apply(Command::Charge { watts: 300 }).await });
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .expect("write must start");
    write.abort();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(control.pending().is_none(), "cancelled write kept pending");
    let next = tokio::time::timeout(Duration::from_secs(3), control.apply(Command::Hold))
        .await
        .expect("next command must not be blocked");
    assert!(next.is_ok(), "next command failed: {:?}", next.err());
}

#[tokio::test]
async fn an_accepted_write_without_a_response_requires_a_verified_hold() {
    let device = Arc::new(Scripted::new(WriteMode::AcceptedNoReply));
    let control = controller(Arc::clone(&device));
    control.poll().await.expect("prime the report");

    let charge = control.apply(Command::Charge { watts: 300 }).await;
    assert!(matches!(
        charge,
        Err(BatteryError::Fault(ControlFault::UncertainWrite { .. }))
    ));
    assert!(matches!(
        control.active_fault(),
        Some(ControlFault::UncertainWrite { .. })
    ));
    assert!(
        control.pending().is_some(),
        "uncertain write lost ownership"
    );

    let ordinary = control.apply(Command::Charge { watts: 100 }).await;
    assert!(matches!(ordinary, Err(BatteryError::Fault(_))));

    let hold = control.apply(Command::Hold).await;
    assert!(hold.is_ok(), "hold failed: {:?}", hold.err());
    for _ in 0..3 {
        control.poll().await.expect("fresh report");
    }
    assert!(
        control.active_fault().is_none(),
        "verified hold did not clear the uncertain fault"
    );
}

/// A non-hold write no longer waits for flow inside the call, but the next read must still turn a
/// command that never delivered into a fault and the 300 s fault backoff.
#[tokio::test(start_paused = true)]
async fn a_deferred_confirmation_still_arms_the_no_flow_backoff() {
    use energy_battery::control::FLOW_CONFIRM_WINDOW;

    let device = Arc::new(Scripted::new(WriteMode::Ready));
    let control = controller(Arc::clone(&device));
    control.poll().await.expect("prime the report");
    control
        .apply(Command::Charge { watts: 300 })
        .await
        .expect("charge accepted");
    assert!(
        control.pending().is_some(),
        "write lost its pending command"
    );

    tokio::time::sleep(FLOW_CONFIRM_WINDOW + Duration::from_secs(1)).await;
    control.poll().await.expect("read after the flow window");
    assert!(
        control.active_fault().is_some(),
        "the fault must survive into the controller state"
    );
    assert!(
        tokio::time::timeout(
            Duration::from_secs(1),
            control.apply(Command::Charge { watts: 100 })
        )
        .await
        .is_err(),
        "the 300 s fault backoff did not block ordinary writes"
    );
}
