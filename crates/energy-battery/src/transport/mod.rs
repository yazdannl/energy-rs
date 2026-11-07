//! zenSDK transport: plain HTTP on port 80, one host per device, no auth, no TLS.

pub mod device_io;
pub mod endpoint;
pub mod http;
pub mod write_ack;
pub mod write_properties;
pub mod write_request;

pub use device_io::{BoxFuture, DeviceIo};
pub use endpoint::{Endpoint, REPORT_PATH, WRITE_PATH};
pub use http::HttpTransport;
pub use write_ack::WriteAck;
pub use write_properties::WriteProperties;
pub use write_request::{MAX_BODY_BYTES, SOURCE_HOME_ASSISTANT, WriteRequest};
