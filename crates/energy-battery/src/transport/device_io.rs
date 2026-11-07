//! The seam the controller and the fleet use: one report, one write, one serial.

use std::future::Future;
use std::pin::Pin;

use crate::error::BatteryError;
use crate::report::Report;
use crate::transport::write_ack::WriteAck;
use crate::transport::write_request::WriteRequest;

/// A boxed future, so the trait stays object-safe without `async fn` in traits.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Everything the control plane needs from a device.
///
/// The real implementation is [`HttpTransport`](crate::HttpTransport); tests script replies and the
/// in-process fake device serves real HTTP.
pub trait DeviceIo: Send + Sync {
    /// Serial number this device answers to (identity for writes and fleet aggregation).
    fn sn(&self) -> &str;

    /// `GET /properties/report`.
    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>>;

    /// `POST /properties/write`.
    fn write(&self, request: WriteRequest) -> BoxFuture<'_, Result<WriteAck, BatteryError>>;
}

impl<T: DeviceIo + ?Sized> DeviceIo for std::sync::Arc<T> {
    fn sn(&self) -> &str {
        (**self).sn()
    }

    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
        (**self).report()
    }

    fn write(&self, request: WriteRequest) -> BoxFuture<'_, Result<WriteAck, BatteryError>> {
        (**self).write(request)
    }
}
