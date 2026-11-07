# Release and packaging

How the add-on is built, published and installed (PLAN.md §10). Two facts drive the whole design:

1. **The Supervisor builds an add-on with the folder that holds `config.yaml` as the Docker build
   context.** That folder (`energy_rs/`) is one level below the Cargo workspace, so a plain
   `docker build` from the add-on folder cannot see `Cargo.toml` or `crates/`.
2. **Compiling Rust on the device is a poor install path**: a Raspberry Pi pulls the builder image
   and spends 5–20 minutes (sometimes OOM with LTO) on every version bump.

## Chosen strategy

Prebuilt per-arch images plus a self-contained staged folder:

| path | context | who uses it |
|---|---|---|
| `docker build -f energy_rs/Dockerfile .` (repository root) | repository root | development, CI, `scripts/build_addon_local.sh` |
| `docker build -f build/energy_rs/Dockerfile build/energy_rs` (staged folder) | staged add-on folder | `scripts/stage_addon.sh`, the release tarball, a Supervisor on-device build |
| `ghcr.io/yazdannl/energy-rs-{arch}:<version>` | — | the Supervisor, because `config.yaml` sets `image:` |

* One Dockerfile, two contexts. The builder copies the whole context (`COPY . .`, `.dockerignore`
  keeps it ~6 MB) and then looks for the entrypoint at both possible paths (`energy_rs/run.sh` in a
  checkout, `run.sh` in a staged folder), so there is no per-context file list to keep in sync. The
  price is Docker's layer cache: a source change recompiles, which is what a release build does
  anyway — iterate with `cargo build`, not `docker build`. **Keep it there:** no BuildKit-only
  features (`RUN --mount`, heredocs) — the Supervisor's on-device builder may use the legacy builder.
* `image:` uses the `{arch}` placeholder (Supervisor substitutes `aarch64`/`amd64`) and the image tag
  is always the manifest `version`, per the add-on docs. Publishing per-arch tags keeps the release
  workflow native; a single multi-arch manifest would need `--push` from one runner and QEMU or a
  manifest merge.
* `scripts/stage_addon.sh` is the only place that knows the staged layout. It also strips `image:`:
  a staged folder exists to be *built*, and the Supervisor would pull the tag instead of building if
  the key were present. It fails when a file the build needs (`crates/`, `web/dist/`, the manifests,
  `run.sh`) is missing, and CI stages the folder on every push (`packaging` job), so the staged
  layout cannot rot unnoticed.

## Cutting a release

```bash
scripts/bump_version.sh 0.2.0     # Cargo.toml + Cargo.lock + energy_rs/config.yaml
cargo test --workspace            # gates (CI runs them again on the tag)
git commit -am "chore(release): 0.2.0"
git tag v0.2.0
git push origin main v0.2.0
```

`.github/workflows/release.yml` then runs, in order:

1. **version** — `scripts/bump_version.sh --check` and `v<tag> == version` (a mismatch stops
   everything before a build starts).
2. **gates** — fmt, clippy `-D warnings`, `cargo test --workspace`, `scripts/check_structure.py`.
3. **image** (matrix) — `npm ci && npm run build`, then `docker buildx build` on native runners:
   `ubuntu-24.04` → `linux/amd64`/`amd64`, `ubuntu-24.04-arm` → `linux/arm64`/`aarch64`, pushed as
   `ghcr.io/yazdannl/energy-rs-amd64:<version>` and `ghcr.io/yazdannl/energy-rs-aarch64:<version>`.
   The arm runners are available to public repositories; for a private fork run the arm row on
   `ubuntu-24.04` with `--platform linux/arm64` and `docker/setup-qemu-action` (much slower).
4. **release** — stages the add-on folder, packs `energy-rs-addon-<version>.tar.gz`, uploads it as an
   artifact and creates the GitHub release with that tarball attached.

Nothing else publishes. A branch push runs `ci.yml` only.

### Credentials this needs (not available here)

| what | where | note |
|---|---|---|
| GHCR push | `GITHUB_TOKEN` with `packages: write` (repository workflow permissions) | already granted in the workflow; the token is created by Actions for the run |
| GitHub release | `GITHUB_TOKEN` with `contents: write` | same run token |
| public pull | GHCR package visibility | after the **first** push, set the two packages to public (Packages → package → Settings → Change visibility), otherwise the Supervisor cannot pull an unauthenticated image |

No local credentials are needed to build. This repository has never pushed an image or created a
release: publishing is a deliberate act on a tag, and the first one is the maintainer's to run.

### Dry run without publishing

Actions → **Release** → *Run workflow*:

* `push_images: false` (default) builds both images, loads them on the runner and smoke-tests
  `GET /api/v1/health`; nothing reaches GHCR.
* `push_images: true` performs exactly what a tag does (both images pushed), which is how to fix a
  registry problem before tagging.

## Local validation

```bash
scripts/build_addon_local.sh                     # stage + docker build, image energy-rs-local:<version>
scripts/build_addon_local.sh --network host      # if the builder has no outbound network (see below)
docker run --rm -p 8099:8099 energy-rs-local:0.1.0
curl -s localhost:8099/api/v1/health             # {"status":"ok","version":"0.1.0"}
```

* The repository-root form (`docker build -f energy_rs/Dockerfile .`) is the one CI and releases use.
* `web/dist` must be built first; the script does it when the folder is missing.
* `docker buildx build --check -f energy_rs/Dockerfile .` (buildx ≥ 0.15) parses both stages without
  building; `hadolint energy_rs/Dockerfile` is a useful extra if installed.
* If the Docker daemon here cannot give build containers a default route (this sandbox: pulls work,
  `apt-get`/crates.io inside the build do not), pass `--network host` to the script. GitHub runners
  do not need it.

## Installing a local build

* **Staged folder in `/addons`.** Copy `build/energy_rs` to `/addons/energy_rs` (Home Assistant OS)
  and the Supervisor offers a local add-on built from source; the tarball from a release is the same
  folder. The staged manifest has no `image:` key — that is intentional.
* **Self-built image locally.** On a Supervised install the image must be in the local Docker daemon
  and `config.yaml` must name it without the published path
  (`image: "energy-rs-local"`), otherwise the Supervisor pulls the registry tag instead.

## Rollback

The Supervisor pulls `<version>` from the manifest. To roll back, edit `version:` in the installed
add-on's `config.yaml` (or install the previous release tarball) and restart. The ledger in `/data`
runs forward-only migrations and refuses to start on a file written by a newer binary — restore the
`/data` backup taken with `backup: cold` together with the older version.
