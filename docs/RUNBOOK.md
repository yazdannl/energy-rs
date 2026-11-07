# Runbook

How to install, start and read the Energy RS add-on. The design is in [PLAN.md](PLAN.md), the HTTP
contract in [API.md](API.md), the storage in [LEDGER.md](LEDGER.md) and the packaging details in
[RELEASE.md](RELEASE.md).

## 1. Install

1. Add the repository `https://github.com/yazdannl/energy-rs` (or the staged folder as a local
   add-on) and install **Energy RS** (`energy_rs`).
2. Fill the options in the add-on configuration page:

| option | meaning |
|---|---|
| `mode` | `advisory` (compute and store plans, write nothing), `dry_run` (the whole pipeline including clamps, intended writes logged, nothing sent), `auto` (writes). Default `dry_run`. |
| `log_level` | `trace` … `error`. Default `info`. |
| `price_provider` | `auto` (direct Zonneplan when a refresh token is set, else the HA sensor), `zonneplan`, `ha_sensor`. |
| `zonneplan_refresh_token` | the app API refresh token. Optional; needed for direct prices. |
| `price_entity` | override the tariff sensor (default `sensor.zonneplan_current_quarter_hourly_electricity_tariff`). |
| `batteries` | a JSON string with manual LAN entries, e.g. `[{"address":"192.168.1.51","serial":"AB2000XXXXXXXX"}]`. mDNS is used in addition unless `ENERGY_RS_MDNS=off`. |

3. Start the add-on. It exposes one HTTP surface on `8099` behind ingress; the Supervisor watchdog
   polls `/api/v1/health`.

The add-on writes `/data/energy_rs.db` (SQLite/WAL: the ledger and the price store). Backups are
`cold`: the Supervisor stops the add-on first, which checkpoints the WAL.

## 2. First boot

Start in `dry_run` (the default). A healthy first boot logs one line like

```
energy-rs ready batteries=4 manual=4 mdns=0 dashboard=true prices=ha_sensor profile_days=0 mode=dry_run
```

and then `plan.generated` events as ticks run (5 s each, a new plan every 15 minutes).

What to check, in order:

1. `GET /api/v1/health` → `{"status":"ok","mode":"dry_run","lastTickAt":"…"}`. A `null`
   `lastTickAt` means no tick completed yet (the log says why).
2. `GET /api/v1/site` → `configured: true` and the dashboard's grid/solar/battery sources. If
   `configured` is false, Home Assistant has no Energy dashboard; every plan is refused with
   `site_not_configured` and the UI shows it.
3. `GET /api/v1/packs` → one row per battery, `state.socPct` moving. `learned` is `null` until a
   charge/discharge session completed and the learner has enough data.
4. `GET /api/v1/prices` → published slots with `buyEurPerKwh` and `feedInEurPerKwh`. A gap in the
   middle is never filled; the tail simply ends at tomorrow's publication.
5. `GET /api/v1/plan/current` → a persisted plan (404 before the first solve). `bindingReasons`
   explains what shaped it (`price_low`, `soc_deviation`, the degraded-forecast note).
6. `GET /api/v1/stream` → SSE: `state` on connect and after every change, `heartbeat` every 15 s.

### Modes in order

* **advisory** — computes and stores plans. No setpoints are even simulated; the dispatch log stays
  empty.
* **dry_run** — runs the full ladder: clamps, grid caps, latch, maintenance. Every tick writes
  `dispatch_log` rows with `verified: false` and the intended power; no HTTP write reaches a device.
* **auto** — the same pipeline, then real writes: `smartMode: 1`, a hold never writes `acMode`, and
  a write is only confirmed when the next report echoes the limit.

Flip modes with `PATCH /api/v1/mode {"mode":"auto"}` or from the UI. The mode is stored in the
ledger (`control.mode`), so a restart resumes it. Leaving `auto` neutralises the packs the add-on
drove; entering `auto` never replays a stale plan (the first tick after a mode change holds until a
fresh plan exists).

## 3. Reading `/api/v1/state`

Everything the UI shows comes from one `EngineSnapshot` published after every tick:

| field | source |
|---|---|
| `mode` | the engine's mode in force |
| `site` | the dashboard discovered by `energy/get_prefs` (sources + capabilities) |
| `plan` | the plan in force: id, horizon, `feasible`, totals, `bindingReasons` |
| `packs[]` | per pack: SoC, power, sample instant, `stale`, limits, learned profile, cell latch, manual override |
| `meter` | grid/PV/load/battery power from the dashboard's live entities |
| `forecast` | PV/load Wh summed over the plan's own slots |
| `lastTick` | last completed tick (epoch seconds) |
| `faults[]` | `no_plan`, `plan_expired`, `pack_unreachable`, `pack_stale`, `meter_missing`, `ha_disconnected`, `prices_stale`, `load_profile` |

A fault is cleared by the engine/loops, never by the reader. The SSE `state` event carries the same
object.

## 4. Troubleshooting

**No dashboard.** `site.configured: false`, plan refused with `site_not_configured`. Configure the
Home Assistant Energy dashboard (Settings → Dashboards → Energy) with at least grid import/export,
solar energy and the battery counters, then wait up to ten minutes (discovery refresh) or restart.

**No devices.** `batteries=0` in the startup line and `site.packs` empty. Check the manual
`batteries` option (address + serial, port defaults to 80) or the LAN/mDNS (`_zendure._tcp`); the
log names an unreachable device once per poll, not once per tick. An unreachable pack is held, not
dropped: the other packs keep working.

**Price gaps.** `prices_stale` fault + `plan.refused: price_gap/price_coverage` events. The direct
provider needs a valid refresh token; the sensor fallback needs the Zonneplan integration and a
sensor with a `forecast` attribute. Before ~13:00 CET tomorrow is not published, so the horizon is
shortened (never forward-filled) and the plan covers the rest of today only.

**No load profile.** The first boot learns it from 56 days of hourly statistics; fewer than seven
days of complete hourly history leaves `profile_days=0`, the `load_profile` fault open and the plan
degraded (residential fallback). It is retried daily and on every settings change.

**The Supervisor restarts the add-on.** The health endpoint never calls HA; a Core restart does not
restart this add-on. If the process does exit, the shutdown path holds the packs (in `auto`),
checkpoints the WAL and exits 0; a hard kill leaves the live-write latch in the ledger, so the next
boot neutralises those packs before doing anything else.

**Reading the journal.** The `events` table is the audit trail (`engine.boot`, `plan.generated`,
`plan.refused`, `dispatch.fault`, `learning.applied`, `mode.changed`, `maintenance.*`). The
`dispatch_log` shows what was intended and what was verified.
