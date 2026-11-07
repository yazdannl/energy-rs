//! Embedded UI assets.
//!
//! `rust-embed` reads `web/dist` at compile time. The folder path is relative to this crate's
//! manifest (`crates/energy-server`), so `../../web/dist/` is the workspace-level build output.
//! A checked-in placeholder `web/dist/index.html` keeps `cargo build` working before the first
//! `npm run build`; when the real build is missing at runtime this handler answers 503 instead of
//! panicking.

use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

/// The Svelte build output (`web/dist`), embedded into the binary.
#[derive(Embed)]
#[folder = "../../web/dist/"]
pub struct WebAssets;

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

/// Serve an embedded file, falling back to `index.html` for client-side routes.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(response) = embedded(path) {
        return response;
    }
    embedded("index.html").unwrap_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "web/dist is embedded at compile time: run `npm run build` in web/ and rebuild",
        )
            .into_response()
    })
}
