//! The plain values behind the add-on options: the options file, the battery list and the two
//! enums that decide which transport and provider the process builds.
//!
//! Separate from [`Options`](crate::bootstrap::options::Options) so the file that reads and
//! validates the document stays small; this module never logs a token.

use std::fmt;
use std::path::Path;

use anyhow::{Context, Result, bail};
use energy_core::safety::Authority;
use serde::Deserialize;

/// One manually entered battery: a LAN address and the device's serial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManualBattery {
    pub(crate) address: String,
    pub(crate) serial: String,
    pub(crate) port: u16,
}

/// How the process reaches Home Assistant.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Transport {
    Supervisor(String),
    Direct { url: String, token: String },
    Absent,
}

impl fmt::Debug for Transport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Supervisor(_) => formatter.write_str("Supervisor(<redacted>)"),
            Self::Direct { url, .. } => {
                write!(formatter, "Direct {{ url: {url}, token: <redacted> }}")
            }
            Self::Absent => formatter.write_str("Absent"),
        }
    }
}

/// Which price source the add-on should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PriceChoice {
    Auto,
    Zonneplan,
    HaSensor,
}

/// The `/data/options.json` document, as far as this binary reads it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct OptionsFile {
    pub(crate) mode: Option<String>,
    pub(crate) log_level: Option<String>,
    pub(crate) price_provider: Option<String>,
    pub(crate) zonneplan_refresh_token: Option<String>,
    pub(crate) price_entity: Option<String>,
    pub(crate) batteries: Option<String>,
    /// The control tick; the add-on option is optional so an older options file stays valid.
    pub(crate) tick_seconds: Option<u64>,
}

/// One battery from the options `batteries` JSON string.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BatteryJson {
    address: Option<String>,
    host: Option<String>,
    serial: Option<String>,
    sn: Option<String>,
    port: Option<u16>,
}

/// Read the options file; a missing file is an empty document (env-only development).
pub(crate) fn read_file(path: &Path) -> Result<OptionsFile> {
    if !path.exists() {
        return Ok(OptionsFile::default());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read add-on options {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse add-on options {}", path.display()))
}

pub(crate) fn env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub(crate) fn env_u16(name: &str) -> Option<u16> {
    env_string(name).and_then(|value| value.parse().ok())
}

pub(crate) fn transport() -> Transport {
    if let Some(token) = env_string("SUPERVISOR_TOKEN").or_else(|| env_string("HASSIO_TOKEN")) {
        return Transport::Supervisor(token);
    }
    match (
        env_string("HASS_URL").or_else(|| env_string("HA_URL")),
        env_string("HASS_TOKEN").or_else(|| env_string("HA_TOKEN")),
    ) {
        (Some(url), Some(token)) => Transport::Direct { url, token },
        _ => Transport::Absent,
    }
}

pub(crate) fn parse_mode(raw: Option<&str>) -> Result<Authority> {
    match raw {
        None => Ok(Authority::DryRun),
        Some(value) => Authority::parse(value.trim()).ok_or_else(|| {
            anyhow::anyhow!("mode must be one of advisory, dry_run, auto (got {value:?})")
        }),
    }
}

pub(crate) fn parse_price(raw: Option<&str>) -> Result<PriceChoice> {
    match raw.map(str::trim) {
        None | Some("" | "auto") => Ok(PriceChoice::Auto),
        Some("zonneplan") => Ok(PriceChoice::Zonneplan),
        Some("ha_sensor") => Ok(PriceChoice::HaSensor),
        Some(other) => bail!("price_provider must be auto, zonneplan or ha_sensor (got {other:?})"),
    }
}

/// Parse the `batteries` JSON string; an entry without an address or serial is refused, not skipped.
pub(crate) fn batteries(raw: Option<&str>) -> Result<Vec<ManualBattery>> {
    let Some(raw) = raw.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(Vec::new());
    };
    let entries: Vec<BatteryJson> = serde_json::from_str(raw)
        .with_context(|| "batteries must be a JSON array of {address, serial, port?}")?;
    entries
        .into_iter()
        .map(|entry| {
            let address = entry
                .address
                .or(entry.host)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
            let serial = entry
                .serial
                .or(entry.sn)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
            match (address, serial) {
                (Some(address), Some(serial)) => Ok(ManualBattery {
                    address,
                    serial,
                    port: entry.port.unwrap_or(80),
                }),
                _ => bail!("every batteries entry needs an address and a serial"),
            }
        })
        .collect()
}
