# Energy RS

Home Assistant add-on for [energy-rs](https://github.com/yazdannl/energy-rs): it reads the Energy
dashboard, plans battery dispatch from prices and the PV/load forecast, and — only in `auto` — writes
setpoints to SolarFlow packs over the LAN.

**State: packaging (P10).** The ledger, prices, battery protocol and planner are in place; the engine
loop and the UI wiring are not (P7–P9). Installed today the add-on serves the ingest UI shell and the
health endpoint, stores its options and logs — `mode` is validated but nothing is dispatched yet, so
`auto` is safe but pointless. Install it side by side with the old add-on, not instead of it.

## Install

### From the add-on repository (prebuilt image, recommended)

Settings → Add-ons → Add-on store → ⋮ → **Repositories** → paste
`https://github.com/yazdannl/energy-rs`, then install **Energy RS** (`energy_rs`).

The manifest pins `image: "ghcr.io/yazdannl/energy-rs-{arch}"`: the Supervisor pulls the published
multi-arch build tagged `version` and never compiles on the device. The image needs to be public in
the package settings, otherwise the pull fails with an authentication error in the Supervisor log.

### From source (no registry)

Both paths below produce the same image; the Dockerfile only reads paths that exist in a repository
checkout *and* in a staged add-on folder.

* **Release tarball.** `energy-rs-addon-<version>.tar.gz` contains a self-contained `energy_rs/`
  folder (Dockerfile, `crates/`, `web/dist/`, manifest). On Home Assistant OS copy it to
  `/addons/energy_rs` (Supervised: to the `addons` folder of the installation) and the Supervisor
  builds it from source on the device — a Rust release build, several minutes and ~2 GB of free
  space, so this is the fallback for a registry you do not want to use. The staged manifest has no
  `image:` key, which is what makes the Supervisor build it.
* **This repository.**
  ```bash
  npm --prefix web ci && npm --prefix web run build   # web/dist is embedded into the binary
  docker build -f energy_rs/Dockerfile -t energy-rs:local .
  docker run --rm -p 8099:8099 energy-rs:local
  curl -s localhost:8099/api/v1/health
  ```
  `scripts/build_addon_local.sh` does the same thing through the staged folder
  (`build/energy_rs/`) that the Supervisor uses, which is the better check: it fails when the
  staged layout drifts from the Dockerfile.

## Options

Options are written to `/data/options.json` before every start. Changing one needs an add-on
restart; there is no hot reload. Every `options:` value must be non-null (the Supervisor rejects an
explicit null with "Missing required option"), so optional settings appear in `schema:` only.

| option | type | default | meaning |
|---|---|---|---|
| `mode` | `advisory` \| `dry_run` \| `auto` | `dry_run` | `advisory` only computes and stores plans; `dry_run` runs the whole pipeline including safety clamps and logs the intended writes; `auto` actually writes to the packs |
| `log_level` | `trace` \| `debug` \| `info` \| `warn` \| `error` | `info` | log filter for the process (exported as `LOG_LEVEL`) |
| `price_provider` | `auto` \| `zonneplan` \| `ha_sensor` | `auto` | `auto` tries the Zonneplan API and falls back to the Home Assistant price sensor |
| `price_entity` | sensor id (optional) | – | the sensor whose `forecast` attribute the fallback provider reads |
| `zonneplan_refresh_token` | password (optional) | – | refresh token for the Zonneplan API. Home Assistant stores option values in `/data/options.json` **in plain text and includes it in backups**; leave it empty and let the `ha_sensor` provider read the Zonneplan integration if that matters to you |
| `batteries` | JSON string | `[]` | manual LAN devices, for a network where mDNS does not reach the packs: `[{"address": "192.168.1.40", "serial": "EOD10XXXXXXXX"}]` |

`auto` is never the default. Move `advisory` → `dry_run` → `auto` one step at a time and compare the
intended setpoints in the log with what the old add-on actually did.

## Permissions and data

| | |
|---|---|
| Home Assistant API | `homeassistant_api: true` — Core REST/WS through `http://supervisor/core` with `SUPERVISOR_TOKEN`. `hassio_api: false`: no Supervisor endpoints beyond the basics. |
| Host folders | `/data` only (`map: data:rw`, which is always mounted): SQLite ledger plus `options.json`. No `config`, `ssl` or `share` access. |
| Network | LAN HTTP to the packs (`:80`), no cloud calls, no account, no token for the batteries. |
| Ports | none exposed. The UI is ingress-only (`ingress_port: 8099`), so it opens from the sidebar and is not reachable from outside Home Assistant. |
| Backups | `backup: cold` — the app is stopped for a backup instead of copying a hot SQLite WAL. |

## Health and logs

* `GET /api/v1/health` → `{"status":"ok","version":"<version>"}`. It answers while the HTTP server is
  alive and never calls Home Assistant, so a Core restart does not make the watchdog restart the
  add-on.
* The manifest watchdog polls `http://[HOST]:[PORT:8099]/api/v1/health`; a container that stops
  answering is restarted by the Supervisor.
* Logs are on the add-on's **Log** tab (`log_level: debug` for detail). Tokens are redacted; no
  option value is ever logged.

## Versioning

`Cargo.toml`, `Cargo.lock` and `energy_rs/config.yaml` carry one version, kept in lockstep by
`scripts/bump_version.sh` (CI refuses drift). The Supervisor pulls the image tag that equals that
version, and a GitHub release tag `v<version>` must match it. See
[docs/RELEASE.md](https://github.com/yazdannl/energy-rs/blob/main/docs/RELEASE.md).

## Files

| file | purpose |
|---|---|
| `config.yaml` | manifest: slug, arch, ingress, watchdog, options/schema, prebuilt image |
| `Dockerfile` | `rust:1.99.1-slim-bookworm` + build-essential builder → `debian:bookworm-slim` runtime |
| `run.sh` | entrypoint: exports `LOG_LEVEL` from the options and `ENERGY_RS_DATA=/data`, then `exec`s the binary so SIGTERM reaches it |
| `icon.png`, `logo.png` | store art, regenerated by `scripts/make-icons.py` |
| this file | published as `DOCS.md` in the staged add-on folder, which is what the store renders |
