//! Embedded UI assets.
//!
//! `rust-embed` reads `web/dist` at compile time. The folder path is relative to this crate's
//! manifest (`crates/energy-server`), so `../../web/dist/` is the workspace-level build output.
//! A checked-in placeholder `web/dist/index.html` keeps `cargo build` working before the first
//! `npm run build`; when the real build is missing at runtime this handler answers 503 instead of
//! panicking.
//!
//! Only the registered SPA routes (and `/`) fall back to `index.html`. A typo in an asset path is a
//! 404 instead of a page of HTML with a 200, which is what a browser cache and a `fetch()` both
//! need.

use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

/// The Svelte build output (`web/dist`), embedded into the binary.
#[derive(Embed)]
#[folder = "../../web/dist/"]
pub struct WebAssets;

/// Client-side routes that must render the SPA shell.
pub const SPA_ROUTES: [&str; 5] = ["plan", "prices", "batteries", "savings", "settings"];

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn embedded(path: &str) -> Option<Response> {
    let asset = WebAssets::get(path)?;
    Some(
        (
            [(header::CONTENT_TYPE, content_type(path))],
            Body::from(asset.data.into_owned()),
        )
            .into_response(),
    )
}

fn index() -> Response {
    embedded("index.html").unwrap_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "web/dist is embedded at compile time: run `npm run build` in web/ and rebuild",
        )
            .into_response()
    })
}

/// Serve an embedded file, the SPA shell for a registered route, or a plain 404.
pub async fn serve(uri: Uri) -> Response {
    let path = uri
        .path()
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_owned();
    if path.is_empty() || SPA_ROUTES.contains(&path.as_str()) {
        return index();
    }
    embedded(&path).unwrap_or_else(|| (StatusCode::NOT_FOUND, "not found").into_response())
}
