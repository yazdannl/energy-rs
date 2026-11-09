//! The add-on options, read from `/data/options.json` with environment overrides.
//!
//! `run.sh` exports `LOG_LEVEL` and `ENERGY_RS_DATA`; everything else comes from the options file the
//! Supervisor writes before every start. Local development may override any value through the
//! `ENERGY_RS_*` variables, and a test may point the process at a temporary data folder and an
//! in-process Home Assistant without touching the file at all.
//!
//! Secrets never leave this type: the refresh token and the HA token are redacted from `Debug`, and
//! nothing here is ever logged.

use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use energy_core::safety::Authority;

use crate::bootstrap::option_value::{
    ManualBattery, PriceChoice, Transport, batteries, env_string, env_u16, parse_mode, parse_price,
    read_file, transport,
};

/// Default ingress port from `energy_rs/config.yaml`.
const DEFAULT_PORT: u16 = 8099;
/// The add-on data volume; `run.sh` sets `ENERGY_RS_DATA`, this is the same default.
const DEFAULT_DATA_DIR: &str = "/data";

/// Everything the runtime is configured with.
pub struct Options {
    data_dir: PathBuf,
    port: u16,
    tick_seconds: u64,
    mode: Authority,
    log_level: String,
    price: PriceChoice,
    refresh_token: Option<String>,
    price_entity: Option<String>,
    batteries: Vec<ManualBattery>,
    mdns: bool,
    scan: bool,
    transport: Transport,
}

impl fmt::Debug for Options {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Options")
            .field("data_dir", &self.data_dir)
            .field("port", &self.port)
            .field("tick_seconds", &self.tick_seconds)
            .field("mode", &self.mode)
            .field("log_level", &self.log_level)
            .field("price", &self.price)
            .field("refresh_token", &"<redacted>")
            .field("price_entity", &self.price_entity)
            .field("batteries", &self.batteries)
            .field("mdns", &self.mdns)
            .field("scan", &self.scan)
            .field("transport", &self.transport)
            .finish()
    }
}

impl Options {
    /// Load the options for the default data folder (`/data`).
    pub fn load() -> Result<Self> {
        let data_dir =
            std::env::var("ENERGY_RS_DATA").unwrap_or_else(|_| DEFAULT_DATA_DIR.to_owned());
        Self::at(data_dir)
    }

    /// Load the options for an explicit data folder (tests, local runs).
    pub fn at(dir: impl Into<PathBuf>) -> Result<Self> {
        let data_dir = dir.into();
        let file = read_file(&data_dir.join("options.json"))?;
        let raw_batteries = env_string("ENERGY_RS_BATTERIES").or(file.batteries);
        let batteries = batteries(raw_batteries.as_deref())?;
        Ok(Self {
            port: env_u16("INGRESS_PORT").unwrap_or(DEFAULT_PORT),
            tick_seconds: 5,
            mode: parse_mode(
                env_string("ENERGY_RS_MODE")
                    .as_deref()
                    .or(file.mode.as_deref()),
            )?,
            log_level: env_string("ENERGY_RS_LOG_LEVEL")
                .or_else(|| env_string("LOG_LEVEL"))
                .or(file.log_level)
                .unwrap_or_else(|| "info".to_owned()),
            price: parse_price(
                env_string("ENERGY_RS_PRICE_PROVIDER")
                    .as_deref()
                    .or(file.price_provider.as_deref()),
            )?,
            refresh_token: env_string("ZONNEPLAN_REFRESH_TOKEN")
                .or(file.zonneplan_refresh_token)
                .filter(|token| !token.trim().is_empty()),
            price_entity: env_string("ENERGY_RS_PRICE_ENTITY")
                .or(file.price_entity)
                .filter(|entity| !entity.trim().is_empty()),
            batteries,
            mdns: env_string("ENERGY_RS_MDNS").as_deref() != Some("off"),
            scan: env_string("ENERGY_RS_SCAN").as_deref() != Some("off"),
            transport: transport(),
            data_dir,
        })
    }

    /// The data volume (ledger, price store, options).
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The database path inside the data volume.
    pub fn database(&self) -> PathBuf {
        self.data_dir.join("energy_rs.db")
    }

    /// The listening address; the Supervisor connects from the Docker network.
    pub fn address(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.port))
    }

    /// The control tick, seconds (tests shorten it; production keeps the shipped 5 s).
    pub fn tick_seconds(&self) -> u64 {
        self.tick_seconds
    }

    /// Set a different control tick.
    #[must_use]
    pub fn with_tick_seconds(mut self, seconds: u64) -> Self {
        self.tick_seconds = seconds.max(1);
        self
    }

    /// The mode the first boot seeds (the stored setting wins after that).
    pub fn mode(&self) -> Authority {
        self.mode
    }

    /// The log filter (`trace`..`error`).
    pub fn log_level(&self) -> &str {
        &self.log_level
    }

    /// Whether mDNS discovery is enabled.
    pub fn mdns(&self) -> bool {
        self.mdns
    }

    /// Whether the LAN is swept for batteries (default on: mDNS is often filtered).
    pub fn scan(&self) -> bool {
        self.scan
    }

    /// The manual batteries, as `(address, serial, port)`.
    pub fn manual_batteries(&self) -> Vec<(String, String, u16)> {
        self.batteries
            .iter()
            .map(|battery| {
                (
                    battery.address.clone(),
                    battery.serial.clone(),
                    battery.port,
                )
            })
            .collect()
    }

    /// `(supervisor mode, base URL, token)`; a clear error when neither mode is configured.
    pub(crate) fn ha(&self) -> Result<(bool, &str, &str)> {
        match &self.transport {
            Transport::Supervisor(token) => Ok((true, "http://supervisor/core", token)),
            Transport::Direct { url, token } => Ok((false, url, token)),
            Transport::Absent => bail!(
                "no Home Assistant transport: set SUPERVISOR_TOKEN (add-on) or HASS_URL and HASS_TOKEN"
            ),
        }
    }

    /// The Zonneplan refresh token, when one is configured.
    pub(crate) fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_deref()
    }

    /// The sensor the HA price fallback should read.
    pub(crate) fn price_entity(&self) -> Option<&str> {
        self.price_entity.as_deref()
    }

    /// Which price provider to construct.
    pub(crate) fn price_choice(&self) -> (&'static str, bool, bool) {
        match self.price {
            PriceChoice::Auto => ("auto", self.refresh_token.is_some(), true),
            PriceChoice::Zonneplan => ("zonneplan", true, false),
            PriceChoice::HaSensor => ("ha_sensor", false, true),
        }
    }

    /// Point the runtime at an explicit Home Assistant (tests, local development).
    #[must_use]
    pub fn with_direct_ha(mut self, url: impl Into<String>, token: impl Into<String>) -> Self {
        self.transport = Transport::Direct {
            url: url.into(),
            token: token.into(),
        };
        self
    }

    /// Add one manually entered battery.
    #[must_use]
    pub fn with_manual_battery(
        mut self,
        address: impl Into<String>,
        serial: impl Into<String>,
        port: u16,
    ) -> Self {
        self.batteries.push(ManualBattery {
            address: address.into(),
            serial: serial.into(),
            port,
        });
        self
    }

    /// Use the given data folder and ingress port.
    #[must_use]
    pub fn with_data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.data_dir = dir.into();
        self
    }

    /// Set the ingress port (0 picks an ephemeral one for a test).
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Seed a different first-boot mode.
    #[must_use]
    pub fn with_mode(mut self, mode: Authority) -> Self {
        self.mode = mode;
        self
    }

    /// Enable or disable mDNS discovery.
    #[must_use]
    pub fn with_mdns(mut self, enabled: bool) -> Self {
        self.mdns = enabled;
        self
    }
}
