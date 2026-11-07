//! One device's local address and serial number.

/// Path of the state report.
pub const REPORT_PATH: &str = "/properties/report";
/// Path of the write endpoint.
pub const WRITE_PATH: &str = "/properties/write";

/// Where a device listens and which serial it answers to.
///
/// There is no credential: the local API is unauthenticated, so the serial in the write body is the
/// only identity check the device performs (a mismatch is rejected with body code 401).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    host: String,
    port: u16,
    sn: String,
}

impl Endpoint {
    /// Default port: a port scan of a SolarFlow found only 80 open.
    pub const DEFAULT_PORT: u16 = 80;

    /// Plain-HTTP endpoint on the default port.
    pub fn new(host: impl Into<String>, sn: impl Into<String>) -> Self {
        Self::with_port(host, Self::DEFAULT_PORT, sn)
    }

    /// Endpoint on an explicit port (mDNS advertises it through SRV, tests use an ephemeral one).
    ///
    /// A pasted `http://` prefix and trailing slashes are tolerated and stripped; a `https://`
    /// prefix is stripped too, because no Zendure device has been seen serving TLS.
    pub fn with_port(host: impl Into<String>, port: u16, sn: impl Into<String>) -> Self {
        Self {
            host: normalize_host(&host.into()),
            port,
            sn: sn.into().trim().to_owned(),
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn sn(&self) -> &str {
        &self.sn
    }

    /// `host:port`, with IPv6 literals bracketed.
    pub fn authority(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// Absolute URL for a path on this device.
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.authority(), path)
    }

    pub fn report_url(&self) -> String {
        self.url(REPORT_PATH)
    }

    pub fn write_url(&self) -> String {
        self.url(WRITE_PATH)
    }
}

fn normalize_host(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_owned()
}
