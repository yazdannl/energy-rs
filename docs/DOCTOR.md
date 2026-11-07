# Read-only diagnostics

`energy-server doctor` and `energy-server plan-once` validate a live installation **before** the
control mode is switched to `auto`. They are diagnostics, not controllers:

* `doctor` opens the ledger and the price store with SQLite `query_only`, reads
  `GET /api/config`, `GET /api/states`, the recorder statistics, `energy/get_prefs` and each device's
  `GET /properties/report`. It never sends `POST /properties/write`, never calls a HA service and
  never changes `control.mode`.
* `plan-once` builds one plan through the real `LivePlanSource` (the same supply the engine uses)
  and prints it. It reads the device reports for a live SoC, does not persist the plan, does not
  dispatch it and does not change the mode.

Neither command prints a token. The Home Assistant token is resolved by `Options` (which redacts it
from `Debug`) and every transport error is redacted by `energy-ha`.

## Running

Inside the add-on container:

```bash
docker exec addon_energy_rs energy-server doctor
docker exec addon_energy_rs energy-server plan-once --json
```

For a local build, with the add-on options file or with environment-only credentials:

```bash
# with /data/options.json (or ENERGY_RS_DATA pointing at it)
cargo run -p energy-server -- doctor --data-dir /data

# direct HA, no /data at all (ledger and price rows then show WARN/FAIL with an explanation)
HASS_URL=http://192.168.1.2:8123 HASS_TOKEN=... \
  cargo run -p energy-server -- doctor --data-dir /tmp/energy-rs
```

Flags:

| flag | meaning |
|---|---|
| `--data-dir <dir>` | data folder for `options.json`, `energy_rs.db`; defaults to `ENERGY_RS_DATA`, else `/data` |
| `--json` | machine-readable JSON instead of the table (both commands) |
| `--verbose` | adds per-phase timings only; no extra checks |

Exit codes:

| command | 0 | 1 | 2 |
|---|---|---|---|
| `doctor` | every check passed | at least one warning, no failure | at least one failure |
| `plan-once` | a plan was solved | — | the plan was refused (reason printed) |

`doctor` also prints `next steps:` for every failed check, with one actionable line per failure.

## Example output

```
STATUS  CHECK                             DETAIL
------  --------------------------------  ------------------------------------------------------------------------------------------------
PASS    ha transport                      direct token, HA 2026.10.0, time zone Europe/Amsterdam, currency EUR
PASS    energy dashboard                  grid sensor.grid_import/sensor.grid_export, grid rate ok, solar sensor.solar_energy,
                                          solar rate ok, forecast entry, batteries 1, device_consumption 0, soc sensor.battery_soc,
                                          capacity 7.68 kWh
PASS    batteries                         manual 1, mDNS 0, devices 1, reachable 1/1
PASS    battery C04A000000000001          solarFlow800Plus, SN match, packs AB2000 1.92 kWh, chargeMaxLimit 1000 W /
                                          inverseMaxPower 800 W, soc 55.0%, ac 0 W, echo age 1s, acMode 2 ok
PASS    stats grid_import 5min/24h        100.0% (288/288), unit kWh (energy), warm-up row ok
PASS    stats grid_import hour/30d        100.0% (720/720), unit kWh (energy), warm-up row ok
...
PASS    live meter                        grid -450 W, PV 1200 W, load 750 W, battery 0 W, soc 55.0%, newest reading 0s old
WARN    prices provider                   auto (direct token: false, HA sensor: true); sensor fallback only
PASS    prices publication                288 slots 11-07 18:30 .. 11-10 18:30 (zonneplan_quarter_hourly), 900s resolution,
                                          provider auto, fetched 0 min ago
PASS    prices coverage                   covers 11-07 18:45 .. 11-09 06:45 (144 slots, 36.2 h ahead); next publication expected
                                          11-08 13:00 CET
PASS    prices derivation                 11-07 18:45 buy 0.20500, market 0.06128 (stored 0.06128), feed-in 0.08128,
                                          export 0.08128 EUR/kWh
PASS    ledger                            schema 5/5, 0.1 MiB, /data/energy_rs.db
PASS    ledger rows                       packs 1, 1m 1, 5m 0, sessions 0, plans 1, dispatch 0, events 0, savings 0
PASS    ledger last tick                  last sample 1s ago, last plan 1s ago, last dispatch -, last event -
PASS    ledger retention                  plans 1/200, dispatch 0/20000, events 0/5000; 1m 30 d, 5m 365 d
WARN    verdict                           would plan: charge 800 W (pv_surplus_charge), soc 1.06 kWh -> 1.24 kWh, buy 0.2000,
                                          feed-in 0.0771 EUR/kWh, 1 pack(s); degraded: load profile missing; residential
                                          fallback shape

26 check(s): 24 pass, 2 warn, 0 fail
timings: transport 56 ms, dashboard 3 ms, batteries 45 ms, statistics 994 ms, prices 10 ms, ledger 5 ms, plan 400 ms,
```

(The table is wrapped here for readability; the program prints one line per check.)

## Reading each row

| row | PASS means | WARN means | FAIL means |
|---|---|---|---|
| `ha transport` | `GET /api/config` answered; the token kind is `supervisor` or `direct` | — | no transport configured, bad URL, 401/403, unreachable Core |
| `energy dashboard` | `energy/get_prefs` resolved, grid import+export and solar energy are named | no battery source, or no solar-forecast entry (the plan is degraded) | no dashboard, or a required statistic is missing |
| `batteries` | every configured/discovered device answered once | some devices are unreachable | no device at all (manual entries and mDNS both empty) |
| `battery <sn>` | report parsed, SN matches, packs labelled, limits read, `acMode` 1 or 2 | SN mismatch, no labelled pack, skewed device clock, `faultLevel >= 2` | unreachable, wedged `acMode` (not 0/1/2), `is_error` set |
| `stats <source> <resolution>` | every bucket present in the window, warm-up row present, unit known | coverage below 98 %, no warm-up row (first bucket is discarded), unit missing | coverage below 50 %, or the entity is not a recorder statistic |
| `live meter` | grid, PV, load and SoC resolved; readings younger than 5 min | one of PV/load/SoC missing, or readings older than 5 min | no grid power reading (no plan is possible) |
| `prices provider` | the configured provider has its credential (direct token) | sensor fallback only | `zonneplan` selected without a refresh token |
| `prices publication` | a 15-minute publication is stored and fetched recently | it ends in the past or is older than 36 h | no publication, unreadable store, or a non-15-minute resolution |
| `prices coverage` | at least one hour of slots ahead (the planner's minimum) | less than six hours ahead (normal before the ~13:00 CET publication) | a gap, or less than one hour ahead |
| `prices derivation` | recomputing `market` from `buy` matches the stored value (1e-6) | the contract constants changed since ingest | the market cannot be derived |
| `ledger` | schema version matches this binary, file readable, path and size shown | — | missing/unreadable file, or a schema this binary cannot read |
| `ledger rows` | packs, samples, plans, dispatch and events are present | the ledger holds no data yet | — |
| `ledger last tick` | a sample and a plan are younger than 5 and 30 minutes | no plan yet, or the engine has not ticked recently | — |
| `ledger retention` | plans/dispatch/events are under their kept limits | a trim is due (over a limit) | — |
| `verdict` | a plan exists at this instant; the detail says **would plan** (charge/discharge W) or **would hold** (a legitimate hold) | the plan is **degraded** (e.g. the residential load fallback), or no plan was attempted | the plan is refused; the reason is printed verbatim |

`stats` rows use 5-minute buckets for the last 24 h and hourly buckets for 30 days. The planner
reads 15-minute buckets over 48 h, so a green 5-minute row is the strongest signal; the hourly row
covers a longer tail (capacity learning and the load profile). The warm-up row is the extra bucket
the statistics parser requests before the window and discards: if the recorder does not return it,
the first bucket of the window is silently dropped, which is why it is flagged.

## plan-once output

The text output starts with the horizon, money and degradation, then one row per 15-minute slot:

```
plan 11-07 18:45 .. 11-09 06:30 (144 slots), objective -1.234567 EUR
cost -1.100000, baseline -0.200000, saved -0.900000 EUR; charged 3.00, discharged 2.60 kWh, 0.55 cycles
grid charge 2.40, PV charge 0.60 kWh, terminal target 3.84 kWh
feasible: yes
degraded: load profile missing; residential fallback shape
SLOT         BUY      FEED-IN  PV kW  LOAD kW  BATTERY W  SOC kWh  REASON
11-07 18:45  0.25000  0.08128  0.00   0.42     +800       1.24     arbitrage_charge
11-07 19:00  0.24500  0.08128  0.00   0.39     +800       1.42     arbitrage_charge
11-07 19:15  0.24000  0.08128  0.00   0.37     0          1.42     reserve_hold
...
```

`--json` prints the same plan as an object:

```json
{
  "generatedAt": 1793808300,
  "horizonStart": 1793808300,
  "horizonEnd": 1793897400,
  "feasible": true,
  "objectiveEur": -1.234567,
  "sellFloorEurPerKwh": 0.0613,
  "summary": { "cost_estimate_eur": -1.1, "saved_eur": -0.9, "...": "..." },
  "degraded": "load profile missing; residential fallback shape",
  "slots": [
    {
      "tStart": 1793808300,
      "buyEurPerKwh": 0.25,
      "feedInEurPerKwh": 0.08128,
      "pvKw": 0.0,
      "loadKw": 0.42,
      "batteryW": 800.0,
      "socKwh": 1.24,
      "reason": "arbitrage_charge"
    }
  ]
}
```

A refusal is exit code 2 and prints/reports the typed reason, for example
`plan refused: price book is stale: 90000s old, limit 64800s`, or `no packs configured`. In JSON the
shape is `{"refused": "...", "packs": 0, "profileDays": 0, "notes": ["..."]}`.

## What it cannot verify without hardware

* **Accepted writes and echoes.** `smartMode`, `acMode` echo, the limit echo and pacing are only
  proven by a real `POST /properties/write`. The doctor reads reports, so it can prove reachability,
  identity, limits and wedge state, not that a write would be accepted.
* **Learned capacity and round-trip efficiency.** These need completed charge/discharge sessions in
  the ledger; the plan uses the learned values when present and the pack label otherwise.
* **mDNS across container boundaries.** The doctor uses the add-on's discovery path; if the container
  cannot receive multicast, neither the doctor nor the add-on will see a device that only answers
  mDNS. Manually entered `batteries` entries avoid this.
* **Price provider credentials.** The doctor reads the stored publication; it does not fetch. An
  expired Zonneplan token shows up as an ageing publication (or the `prices_stale` fault in the UI),
  not as a credential check.
* **Forecast accuracy.** The verdict uses the real model/fallback, but a green row says the inputs
  are present, not that tomorrow's weather is right.
* **Recorder retention policies you cannot see.** A statistic can exist and still be excluded by the
  recorder; the coverage rows show exactly that.
* **A ledger from a newer schema.** Doctor refuses it (`FAIL ledger`); it never migrates. Start the
  matching add-on version first.
