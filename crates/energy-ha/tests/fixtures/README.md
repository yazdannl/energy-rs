# Recorded HA fixtures (scrubbed)

Captured read-only on 2026-11-07 from HA Core 2026.10.4 with the `hassio` CLI, then scrubbed.
Every entity id is `sensor.example_NNN`, every device/serial/uuid/config-entry id is fake;
names, coordinates and URLs are replaced. Numeric values and JSON shape are real.

| file | source |
|---|---|
| energy_prefs.json | `ws call -T energy/get_prefs` |
| energy_prefs_not_configured.json | documented `not_found` error |
| solar_forecast.json | `ws call -T energy/solar_forecast` |
| core_config.json | `GET /api/config` |
| device_registry.json | `ws call -T config/device_registry/list` (4 of 601) |
| statistics_metadata.json | `recorder/get_statistics_metadata` for 23 ids |
| statistic_ids.json | `recorder/list_statistic_ids` (sum) |
| statistics_5minute.json | `recorder/statistics_during_period`, 5minute, 3 h |
| statistics_hourly.json | `recorder/statistics_during_period`, hour, 2 d |
| history.json | `history/history_during_period`, minimal_response |
| states.json | `GET /api/states/<id>` for 4 ids |
| subscribe_events.json | `ws subscribe --event-type state_changed` |
| subscribe_entities.json | raw a/c frames re-encoded from a real `observe-entities` capture |
