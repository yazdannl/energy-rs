# energy-ledger: schema, retention and session rules

`crates/energy-ledger` owns one SQLite file (default `/data/energy_rs.db`, WAL,
`foreign_keys=ON`, `synchronous=NORMAL`, 5 s busy timeout) and is the only writer of the tables
below. Money is EUR rounded to 1e-6 on write, energy is kWh (f64), time is UTC epoch seconds, local
days are `Europe/Amsterdam` dates (`YYYY-MM-DD`).

## Table ownership

| owner | tables |
|---|---|
| `energy-ledger` | `packs`, `measurements`, `measurements_5m`, `sessions`, `plans`, `plan_slots`, `dispatch_log`, `events`, `savings_daily`, `meta`, `settings`, `settings_history` |
| `energy-price` | `price_publications`, `price_slots` (P3) |
| `energy-forecast` | forecast caches/archives when P4 needs them |

The ledger does not create, migrate or write the price/forecast tables. Both stores may share the
file: migrations are additive and each table name is owned by exactly one crate.

## Tables

* `packs` — `sn` (unique), optional name, label and reported capacity, optional device power
  maxima, `first_seen`/`last_seen`, and the applied learning numbers (`learned_capacity_kwh`,
  `learned_round_trip`, `learned_confidence`, `learned_at`, migration v4). The upsert keeps
  `first_seen`, moves `last_seen` forward, never blanks a known limit or name with `NULL` and never
  touches the learned columns; only a learning pass writes those.
* `measurements` — 1-minute samples keyed `(pack_id, ts)`; `power_w` signed (positive charges the
  pack), optional `soc_pct`, `kind` (`report` or `derived`). Ingestion is an upsert, so a repeated
  poll is idempotent.
* `measurements_5m` — rollup of the 1-minute tier, keyed `(pack_id, ts)` with 5-minute bucket
  starts. `power_w`/`soc_pct` are means, `samples` is the count; a bucket with no 1-minute rows is
  stored explicitly with `NULL` means and `samples = 0`.
* `sessions` — extracted charge/discharge runs (below). No `session_samples` link table: the
  1-minute tier and the year of 5-minute rows are the sample record, and the run is fully described
  by its start/end, count and energy.
* `plans` / `plan_slots` — every generated plan (header + `summary_json`) and its 15-minute
  timeline; one transaction per plan.
* `dispatch_log` — every requested/applied write with `verified` (echo check) and `error`, including
  dry runs and rejections.
* `events` — append-only journal: level (`debug`/`info`/`warn`/`error`), machine `kind`, message and
  an opaque `data_json`.
* `savings_daily` — one row per local day with actual/baseline/saving, the four-way split
  (`arbitrage_eur`, `self_consumption_eur`, `feed_in_eur`, `battery_balance_eur`), import/export/PV/
  battery energy, `coverage_hours` and `complete` (v5). Money is `NULL` for a closed day whose slots
  were not all measured and for any row written before v5 (its per-slot detail was never stored);
  the split columns of such a row are `NULL` too. The current day is provisional: money present,
  `complete = 0`.
* `meta` — `schema_version` plus run metadata.
* `settings` / `settings_history` — current value per key and the append-only history used for
  as-of reads. Setting the same value again only moves `updated_at`.

## Read-only access

`Ledger::open_read_only(path)` opens an existing file with `PRAGMA query_only = ON`: no creation, no
migration, no trim, no pack registration. It refuses a missing file and a schema newer than the
binary. `LedgerStats::read(path)` builds on it for the doctor: schema version, file size, per-table
row counts and the newest measurement/plan/dispatch/event timestamp. The price store has the same
`SqlitePriceRepository::open_read_only` entry point, so a diagnostic never moves a file forward.

## Migrations

Forward-only, numbered, in code (`db/migrations/vN.rs`), applied inside one transaction each and
recorded in `meta.schema_version`. `meta` is created before the first migration, so an empty file,
a file at any prior version and an already-current file all take the same path. A database whose
version is newer than the binary is refused (`SchemaTooNew`) instead of opened. Tests migrate from
empty and from every prior version. v3 added the dispatch note; v4 added the four `learned_*` pack
columns; v5 rebuilt `savings_daily` with nullable money and the four split columns, copying existing
rows (money kept, splits `NULL`). `Ledger::checkpoint()` runs `PRAGMA wal_checkpoint(TRUNCATE)` on the graceful stop path so
the cold backup sees a complete file.

## Retention

`Ledger::run_retention(now)` / `run_retention_now()` runs in one transaction:

| data | kept |
|---|---|
| 1-minute measurements | 30 days (`ts >= now - 30 d`, boundary row kept) |
| 5-minute measurements | 1 year |
| plans + slots | newest 200 |
| dispatch log | newest 20 000 |
| sessions, events, savings_daily | forever |

No trim deletes a row inside the current local day, including the count trims (today's plans are
never the ones dropped). Empty-name `RetentionPolicy` fields are configurable per `LedgerConfig`.
The job returns per-table deleted counts and an immediate re-run returns zeros.

The 5-minute rollup only overwrites a populated bucket when the recomputation actually saw
1-minute samples, so running the rollup over a window whose raw rows have already been trimmed
cannot erase the year of 5-minute history.

## Session extraction

`Sessions::extract(pack_id, tier, from, to, rules)` reads the chosen tier (1-minute or 5-minute)
plus one sample of context on each side, deletes the sessions whose `started_at` lies in the window
and rewrites them in one transaction. It is deterministic and re-runnable.

* A sample is a charge when `power_w >= deadband`, a discharge when `power_w <= -deadband`, and a
  hold otherwise. Runs are maximal same-direction stretches of at least `min_samples` samples.
* A timestamp gap larger than `max_gap_intervals × interval` splits the input; the pieces are then
  unbounded and therefore incomplete.
* `energy_ac_kwh = |mean power| × covered seconds`; the run's leading partial interval is never
  claimed, and a bounded run's trailing edge ends at the next contiguous sample. A run that runs
  into the window edge is credited only to its last sample plus one nominal interval.
* `complete` is true only when a hold or an opposite sample bounds the run on both sides and every
  sample carries a SoC. `invalidated_reason` is `soc_gap` or `direction_reversal`; flagged rows are
  kept for audit but must be excluded from learning (they are not "usable" sessions).
* A one-sample opposite-direction blip between two runs of the same direction is not booked; both
  neighbouring runs are flagged `direction_reversal`.
* Two samples are the minimum, and `soc_start`/`soc_end` come from the first/last sample of the
  run (never interpolated).

Defaults: 20 W deadband, 2 samples, 2 intervals of gap tolerance. `Direction::Charge` means power
into the battery.
