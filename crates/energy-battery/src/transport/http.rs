//! Plain-HTTP client for one device.
//!
//! No TLS (never seen on these devices), no proxy (a proxy would route LAN traffic off-box), one
//! request at a time per device (the embedded server is serial), and a per-device timeout.

use std::time::Duration;

use reqwest::StatusCode;

use crate::error::BatteryError;
use crate::report::{Report, parse_report};
use crate::transport::device_io::{BoxFuture, DeviceIo};
use crate::transport::endpoint::Endpoint;
use crate::transport::write_ack::WriteAck;
use crate::transport::write_request::WriteRequest;

/// A live report is ~2 KB; the cap only defends against a misbehaving responder.
const MAX_REPORT_BYTES: usize = 64 * 1024;
const CONTENT_TYPE: &str = "application/json; charset=UTF-8";

/// reqwest client bound to one device endpoint.
pub struct HttpTransport {
    client: reqwest::Client,
    endpoint: Endpoint,
}

impl HttpTransport {
    /// Default timeout: the vendor integration uses 2 s, the reference library 5 s, field reports
    /// 8 s. 5 s is the middle and still well inside a control tick.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

    pub fn new(endpoint: Endpoint) -> Result<Self, BatteryError> {
        Self::with_timeout(endpoint, Self::DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(endpoint: Endpoint, timeout: Duration) -> Result<Self, BatteryError> {
        if endpoint.sn().is_empty() {
            return Err(BatteryError::Invalid {
                reason: format!("device {} has no serial number", endpoint.authority()),
            });
        }
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .no_proxy()
            .user_agent("energy-rs")
            .build()
            .map_err(|err| BatteryError::Transport {
                host: endpoint.host().to_owned(),
                port: endpoint.port(),
                reason: err.to_string(),
            })?;
        Ok(Self { client, endpoint })
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// `GET /properties/report`, tolerantly parsed.
    pub async fn fetch_report(&self) -> Result<Report, BatteryError> {
        let response = self
            .client
            .get(self.endpoint.report_url())
            .send()
            .await
            .map_err(|err| self.transport_error(&err))?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|err| self.transport_error(&err))?;
        if !status.is_success() {
            return Err(self.status_error(status, &body));
        }
        if body.len() > MAX_REPORT_BYTES {
            return Err(BatteryError::Payload {
                reason: format!("report is {} bytes, cap is {MAX_REPORT_BYTES}", body.len()),
            });
        }
        let report = parse_report(&body)?;
        if report.sn != self.endpoint.sn() {
            return Err(BatteryError::SnMismatch {
                sent: self.endpoint.sn().to_owned(),
                got: report.sn,
            });
        }
        Ok(report)
    }

    /// `POST /properties/write`, with the 512-byte body cap enforced before sending.
    pub async fn send_write(&self, request: &WriteRequest) -> Result<WriteAck, BatteryError> {
        let body = request.body()?;
        let response = self
            .client
            .post(self.endpoint.write_url())
            .header(reqwest::header::CONTENT_TYPE, CONTENT_TYPE)
            .body(body)
            .send()
            .await
            .map_err(|err| self.transport_error(&err))?;
        let status = response.status();
        let reply = response
            .bytes()
            .await
            .map_err(|err| self.transport_error(&err))?;
        if !status.is_success() {
            return Err(self.status_error(status, &reply));
        }
        WriteAck::parse(&reply, request.sn())
    }

    fn transport_error(&self, err: &reqwest::Error) -> BatteryError {
        BatteryError::Transport {
            host: self.endpoint.host().to_owned(),
            port: self.endpoint.port(),
            reason: err.to_string(),
        }
    }

    fn status_error(&self, status: StatusCode, body: &[u8]) -> BatteryError {
        // A 401 on the HTTP layer means "local API switched off", not "wrong token": there is none.
        if status == StatusCode::UNAUTHORIZED {
            return BatteryError::Disabled {
                host: self.endpoint.host().to_owned(),
            };
        }
        if status == StatusCode::BAD_REQUEST || status == StatusCode::PAYLOAD_TOO_LARGE {
            match WriteAck::parse(body, self.endpoint.sn()) {
                Err(typed) if is_rejection(&typed) => return typed,
                _ => {}
            }
        }
        BatteryError::HttpStatus {
            host: self.endpoint.host().to_owned(),
            port: self.endpoint.port(),
            status: status.as_u16(),
        }
    }
}

/// A refusal the body explained, as opposed to one the transport invented.
fn is_rejection(error: &BatteryError) -> bool {
    matches!(
        error,
        BatteryError::Rejected { .. }
            | BatteryError::SnMismatch { .. }
            | BatteryError::BufferOverflow { .. }
    )
}

impl DeviceIo for HttpTransport {
    fn sn(&self) -> &str {
        self.endpoint.sn()
    }

    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
        Box::pin(self.fetch_report())
    }

    fn write(&self, request: WriteRequest) -> BoxFuture<'_, Result<WriteAck, BatteryError>> {
        Box::pin(async move { self.send_write(&request).await })
    }
}
