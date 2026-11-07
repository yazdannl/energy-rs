# web/ — Energy RS UI

Svelte 5 + TypeScript + Vite + Tailwind 4. Build output goes to `web/dist`.

```bash
NODE_OPTIONS=--max-old-space-size=4096 npm install
NODE_OPTIONS=--max-old-space-size=4096 npm run build
```

## dist is embedded in the Rust binary

`crates/energy-server/src/assets.rs` pulls `../../web/dist/` in with `rust-embed` at **compile
time**, so:

* `cargo build` embeds whatever `web/dist` contains at that moment; build the UI first for a real
  binary.
* a checked-in placeholder `web/dist/index.html` keeps `cargo build` working on a fresh clone,
  before the first `npm run build`. Everything else in `web/dist` is git-ignored.
* `vite.config.ts` sets `base: './'` so the same build works behind the Home Assistant ingress path
  (`/api/hassio_ingress/<token>/`); no absolute `/assets/...` URLs.
* the UI calls `./api/v1/health`, i.e. relative to the document, which the server also serves.
