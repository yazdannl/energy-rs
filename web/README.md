# web/ — Energy RS UI

Svelte 5 (runes) + TypeScript + Vite + Tailwind 4 + uPlot. Build output goes to `web/dist`.

```bash
NODE_OPTIONS=--max-old-space-size=4096 npm install
npm run dev            # http://localhost:5173/?mock=1  (fixtures, no backend needed)
npm run lint           # eslint 9 flat config
npm run format:check   # prettier, printWidth 100
npm run check          # svelte-check, zero errors
npm run test           # vitest: format/data helpers
NODE_OPTIONS=--max-old-space-size=4096 npm run build
```

## Mock mode

`?mock=1` (before or after the hash) swaps the HTTP client and the SSE stream for fixtures in
`src/lib/api/mock/`: 48 h of quarter-hour prices, a 30 h plan, two packs, savings rows, settings and
a live tick every four seconds. The mode is explicit — a failed fetch is never silently replaced by
fixtures — and the UI badges itself `mock`.

## Screens

Hash routes (`#/`, `#/plan`, `#/prices`, `#/batteries`, `#/savings`, `#/settings`), one component
per screen in `src/routes/`, domain components in `src/components/<domain>/`, the shared kit in
`src/components/ui/`. No component is longer than 250 lines (`scripts/check_structure.py`).

## Data layer

- `src/lib/api/types.ts` — the `docs/API.md` contract, field for field (camelCase, units in names).
- `src/lib/api/client.ts` — `ApiPort` + `HttpApi`; every URL is built from `document.baseURI`, so
  `./api/v1/…` works at the add-on root and behind the ingress path. The API's error envelope
  becomes a typed `ApiError` whose `code` and `message` are safe to show.
- `src/lib/api/stream.ts` — `EventSource` on `./api/v1/stream`, explicit reconnect with backoff
  (1 → 15 s). A `plan` event carries only an id; the page re-reads `/plan/current`.
- `src/lib/format/` — money (6-decimal view), energy (Wh/kWh/W), sign conventions (positive battery
  power charges, positive grid power imports), the settlement preview formula mirrored from
  `crates/energy-price/src/settlement/`, and local-time labels.

## dist is embedded in the Rust binary

`crates/energy-server/src/assets.rs` pulls `../../web/dist/` in with `rust-embed` at **compile
time**, so:

- `cargo build` embeds whatever `web/dist` contains at that moment; build the UI first for a real
  binary.
- a checked-in placeholder `web/dist/index.html` keeps `cargo build` working on a fresh clone,
  before the first `npm run build`. Everything else in `web/dist` is git-ignored.
- `vite.config.ts` sets `base: './'` so the same build works behind the Home Assistant ingress path
  (`/api/hassio_ingress/<token>/`); no absolute `/assets/...` URLs.
- the UI calls `./api/v1/health`, i.e. relative to the document, which the server also serves.
