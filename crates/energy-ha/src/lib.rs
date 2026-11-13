//! Home Assistant integration.
//!
//! Responsibility (PLAN.md §2, §7): everything that talks to Core.
//!
//! * [`client`] — REST and WebSocket access, auth handshake, typed errors, timeouts and
//!   reconnect/restartable supervision.
//! * [`discovery`] — resolve the energy dashboard configuration (`energy/get_prefs`) into the
//!   grid/solar/load/battery sensor set this app needs, including live-power rules and SoC.
//! * [`stats`] — long-term statistics and history reads, using `change` (not `sum`) for energy
//!   counters, with explicit gaps.
//! * [`subscribe`] — live state subscriptions (`subscribe_entities`, with the
//!   `subscribe_events` fallback) feeding the engine's slot measurements.
//! * [`state`] — the shared entity-state model used by REST, history and subscriptions.
//!
//! In the add-on the base URL is `http://supervisor/core` with `SUPERVISOR_TOKEN`; the same code
//! works against a plain HA URL with a long-lived token for local development.
//!
//! The token is never logged and is redacted from error messages. This crate is read-only: it has
//! no service-call or preference-write helpers (those belong to an explicit, separate write path).

pub mod client;
pub mod discovery;
pub mod state;
pub mod stats;
pub mod subscribe;
pub mod weather;

pub use client::{
    Config, Connection, CoreConfig, EventStream, HaClient, HaError, LinkState, ReconnectPolicy,
    Supervisor,
};
pub use discovery::{
    BatteryModel, BatteryPrefs, DeviceConsumption, DeviceModel, DeviceRegistryEntry,
    EnergyPreferences, EnergySource, GridModel, GridPrefs, Live, PowerConfig, PowerReading,
    PowerRule, SiteModel, SolarForecast, SolarForecastEntry, SolarModel, SolarPrefs,
    device_registry, discover, fetch_prefs, fetch_solar_forecast, to_watts,
};
pub use state::{EntityMap, EntityState};
pub use stats::{
    Granularity, HistoryQuery, HistoryResponse, HistoryState, Period, Slot, StatRow, StatType,
    StatisticKind, StatisticMetadata, StatisticsQuery, StatisticsResponse, history_during_period,
    list_statistic_ids, mean_series_hourly, parse_slots, slot_series, slot_series_5min,
    slot_series_15min, slot_series_hourly, statistic_metadata_for, statistics_during_period,
    statistics_metadata,
};
pub use subscribe::{
    CompressedDiff, CompressedPatch, CompressedState, EntityDiffEvent, EntityStore, EntityUpdate,
    EntityWatch, watch_entities,
};
pub use weather::weather_forecast_hourly;
