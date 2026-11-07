//! Energy dashboard discovery and live-power resolution.

mod battery_model;
mod battery_prefs;
mod device_consumption;
mod device_model;
mod device_registry_entry;
mod energy_preferences;
mod energy_source;
mod grid_model;
mod grid_prefs;
mod live;
mod power_config;
mod power_reading;
mod power_rule;
mod site_model;
mod solar_forecast;
mod solar_model;
mod solar_prefs;

pub use battery_model::BatteryModel;
pub use battery_prefs::BatteryPrefs;
pub use device_consumption::DeviceConsumption;
pub use device_model::DeviceModel;
pub use device_registry_entry::{DeviceRegistryEntry, device_registry};
pub use energy_preferences::{EnergyPreferences, discover, fetch_prefs};
pub use energy_source::EnergySource;
pub use grid_model::GridModel;
pub use grid_prefs::GridPrefs;
pub use live::Live;
pub use power_config::PowerConfig;
pub use power_reading::{PowerReading, to_watts};
pub use power_rule::PowerRule;
pub use site_model::SiteModel;
pub use solar_forecast::{SolarForecast, SolarForecastEntry, fetch_solar_forecast};
pub use solar_model::SolarModel;
pub use solar_prefs::SolarPrefs;
