mod common;

use common::result;
use energy_ha::{
    CoreConfig, DeviceRegistryEntry, EnergyPreferences, EnergySource, EntityState, HistoryResponse,
    SolarForecast, StatisticMetadata, StatisticsResponse,
};

#[test]
fn energy_prefs_fixture_parses_grid_and_solar() {
    let prefs: EnergyPreferences =
        serde_json::from_value(result("energy_prefs.json")).expect("prefs");
    assert_eq!(prefs.energy_sources.len(), 7);
    assert_eq!(
        prefs
            .energy_sources
            .iter()
            .filter(|source| matches!(source, EnergySource::Unknown))
            .count(),
        1,
        "gas is parsed as an unknown source and skipped"
    );

    let grid = prefs
        .energy_sources
        .iter()
        .find_map(|source| match source {
            EnergySource::Grid(grid) => Some(grid),
            _ => None,
        })
        .expect("grid source");
    assert!(grid.stat_energy_from.is_some());
    assert!(grid.stat_energy_to.is_some());
    assert!(grid.stat_rate.is_some());
    let power = grid.power_config.as_ref().expect("grid power_config");
    assert!(power.stat_rate_from.is_some());
    assert!(power.stat_rate_to.is_some());

    let solar = prefs
        .energy_sources
        .iter()
        .find_map(|source| match source {
            EnergySource::Solar(solar) => Some(solar),
            _ => None,
        })
        .expect("solar source");
    assert_eq!(
        solar.config_entry_solar_forecast.as_ref().map(Vec::len),
        Some(2)
    );
}

#[test]
fn energy_prefs_fixture_parses_batteries_and_devices() {
    let prefs: EnergyPreferences =
        serde_json::from_value(result("energy_prefs.json")).expect("prefs");
    let batteries: Vec<_> = prefs
        .energy_sources
        .iter()
        .filter_map(|source| match source {
            EnergySource::Battery(battery) => Some(battery),
            _ => None,
        })
        .collect();
    assert_eq!(batteries.len(), 4);
    for battery in &batteries {
        assert!(battery.stat_energy_from.is_some(), "discharge");
        assert!(battery.stat_energy_to.is_some(), "charge");
        assert!(battery.stat_soc.is_some());
        assert_eq!(battery.capacity, Some(2.06));
        assert!(battery.stat_rate.is_none());
        assert!(battery.power_config.is_none());
    }

    assert_eq!(prefs.device_consumption.len(), 1);
    let device = &prefs.device_consumption[0];
    assert!(device.stat_consumption.is_some());
    assert!(device.stat_rate.is_some());
    assert!(device.included_in_stat.is_none());
}

#[test]
fn solar_forecast_fixture_parses() {
    let forecast: SolarForecast =
        serde_json::from_value(result("solar_forecast.json")).expect("forecast");
    assert_eq!(forecast.len(), 1);
    let entry = forecast.values().next().expect("entry");
    assert_eq!(entry.wh_hours.len(), 24);
    assert!(entry.wh_hours.values().any(|value| *value > 0.0));
}

#[test]
fn metadata_and_statistic_id_fixtures_parse() {
    let metadata: Vec<StatisticMetadata> =
        serde_json::from_value(result("statistics_metadata.json")).expect("metadata");
    assert_eq!(metadata.len(), 23);
    assert_eq!(metadata.iter().filter(|item| item.has_sum).count(), 13);
    assert_eq!(metadata.iter().filter(|item| item.has_mean).count(), 10);
    assert!(
        metadata
            .iter()
            .any(|item| item.unit_class.as_deref() == Some("power"))
    );

    let ids: Vec<StatisticMetadata> =
        serde_json::from_value(result("statistic_ids.json")).expect("statistic ids");
    assert_eq!(ids.len(), 71);
    assert!(ids.iter().all(|item| item.has_sum));
}

#[test]
fn statistics_fixtures_parse_with_change_rows() {
    let five: StatisticsResponse =
        serde_json::from_value(result("statistics_5minute.json")).expect("5minute");
    assert_eq!(five.len(), 23);
    assert_eq!(five.values().map(Vec::len).sum::<usize>(), 828);
    assert!(
        five.values()
            .flatten()
            .any(|row| row.change.is_some_and(|change| change > 0.0))
    );

    let hourly: StatisticsResponse =
        serde_json::from_value(result("statistics_hourly.json")).expect("hourly");
    assert_eq!(hourly.len(), 23);
    assert_eq!(hourly.values().map(Vec::len).sum::<usize>(), 1060);
    assert!(hourly.values().flatten().any(|row| row.sum.is_some()));
}

#[test]
fn history_fixture_parses_minimal_response() {
    let history: HistoryResponse = serde_json::from_value(result("history.json")).expect("history");
    assert_eq!(history.len(), 2);
    let rows: Vec<_> = history.values().flatten().collect();
    assert_eq!(rows.len(), 10);
    assert!(rows.iter().all(|row| row.state.is_some()));
    assert!(rows.iter().all(|row| row.last_updated.is_some()));
}

#[test]
fn device_registry_fixture_parses() {
    let devices: Vec<DeviceRegistryEntry> =
        serde_json::from_value(result("device_registry.json")).expect("devices");
    assert_eq!(devices.len(), 4);
    assert!(devices.iter().any(|device| device.serial_number.is_some()));
    assert!(devices.iter().any(|device| !device.connections.is_empty()));
    assert!(devices.iter().any(|device| device.via_device_id.is_some()));
    assert!(
        devices.iter().all(|device| {
            matches!(device.manufacturer.as_deref(), Some("Example Corp") | None)
        })
    );
}

#[test]
fn rest_config_and_states_fixtures_parse() {
    let config: CoreConfig =
        serde_json::from_value(common::fixture("core_config.json")).expect("config");
    assert_eq!(config.currency, "EUR");
    assert_eq!(config.time_zone, "Europe/Amsterdam");
    assert!(!config.version.is_empty());

    let states: Vec<EntityState> =
        serde_json::from_value(common::fixture("states.json")).expect("states");
    assert_eq!(states.len(), 4);
    assert!(states.iter().all(|state| state.last_updated.is_some()));
    assert!(states.iter().all(|state| state.unit().is_some()));
    assert!(
        states
            .iter()
            .filter(|state| state.numeric().is_some())
            .count()
            >= 3
    );
}
