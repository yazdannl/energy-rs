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

### Safety invariants

These hold for every tick, including the ticks that fail. They are enforced in `crates/energy-core`
and pinned by the tests in `crates/energy-core/tests/`.

* **One gate for every hardware operation.** No command reaches a device except through the engine's
  gated sink, which checks authority, control generation and durable ownership, in that order.
  `advisory` and `dry_run` write nothing at all -- a hold is a write too -- so a `dry_run` restart never
  neutralises what a previous `auto` run left behind: those packs stay latched and are neutralised the
  moment control enters `auto`. Leaving `auto` fences the generation and holds every latched pack
  *while still in `auto`*.
* **A superseded command never reaches a device.** A tick captures a token (mode + generation) before
  it reads anything; an emergency hold, a shutdown or a mode change bumps the generation first, so a
  command queued by an overtaken tick is refused immediately before the transport write.
* **Ownership is durable before the write.** A non-zero setpoint is persisted into the live-write latch
  (`control.live_writes`) *before* the transport call, and a journal failure means the command is not
  sent. Only a verified hold clears an entry, and the latch is persisted before the in-memory entry is
  dropped, so a bookkeeping failure never loses track of a pack that was driven.
* **The final command vector is what the limits bound.** The allocation clamp runs before the per-pack
  ladder, so a second fleet-level pass validates the summed vector -- overrides, forced emergency
  charges and ramp bounds included -- against the fleet power caps and the site import/export
  ceilings, counting the measured power of the packs the tick does not touch.
* **The watchdog needs successful progress.** Progress is marked after successful reads, writes and
  tick steps, never at tick start; a failed tick journals the cause and best-effort holds the packs the
  process owns. Stall detection compares the epoch and a monotonic anchor, so neither a long tick nor a
  backwards wall clock hides a stall.
* **Nothing is forgotten.** A pack that vanished from the read, or reports itself unavailable, is held
  and stays latched until a device confirms; a hold that fails for one pack never stops the others.
* **Unknown state means no setpoint.** With neither stored energy nor SoC, a pack gets at most the
  neutral hold (`state_missing`), in both directions.
* **A corrupt latch is a recovery error.** It is never replaced by an empty list: the stored value is
  preserved, the engine keeps running `dry_run` and refuses `auto` until an operator reconciles it
  (`Engine::reconcile_latch`).

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

## 4. Read-only diagnostics (before `auto`)

The binary also accepts two subcommands that validate an install without touching anything. Both use
the same options file/environment as the add-on, print no token and never write to Home Assistant or
a battery:

```bash
docker exec addon_energy_rs energy-server doctor
docker exec addon_energy_rs energy-server plan-once --json
```

`doctor` exits `0` when every check passed, `1` for warnings only and `2` for at least one failure;
it prints one aligned row per check plus `next steps` for failures:

```
PASS    ha transport                      direct token, HA 2026.10.0, time zone Europe/Amsterdam, currency EUR
PASS    stats grid_import 5min/24h        100.0% (288/288), unit kWh (energy), warm-up row ok
PASS    battery C04A000000000001          solarFlow800Plus, SN match, packs AB2000 1.92 kWh, ...
PASS    prices coverage                   covers ... (144 slots, 36.2 h ahead); next publication expected 13:00 CET
WARN    verdict                           would hold: no battery move in this slot (reserve_hold), ...
```

The rows cover the HA transport and token kind, the energy dashboard (`energy/get_prefs`), statistics
coverage at 5-minute/24 h and hourly/30 d for every source (including warm-up-row risk and missing
units), the live grid/PV/load balance with staleness, the price publication/coverage/derivation, each
battery's report (reachable, SN match, product, pack labels, `chargeMaxLimit`/`inverseMaxPower`, SoC,
power, echo age, wedge state), the ledger (path, schema, row counts, last tick/plan age, retention
headroom) and a **would plan / would hold** verdict for this moment with the reason.

`plan-once` solves one plan through the real `LivePlanSource` and prints the 15-minute timeline
(`slot, buy, feed-in, pv, load, battery W, soc, reason`) with the summary and any degraded or refusal
reason; it exits `2` when the plan is refused and never persists the plan or changes the mode.
`--json` is for scripting, `--verbose` adds timings only, and `--data-dir` points at a non-default
`/data` folder. Every row and the limits of a read-only check are documented in
[DOCTOR.md](DOCTOR.md).

## 5. Troubleshooting

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
