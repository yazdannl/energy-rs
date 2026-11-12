//! The `/api/v1` router.
//!
//! Every route is read-only except `PATCH /mode`, `PATCH /settings`, `PATCH /packs/{sn}` and the
//! three command POSTs, which validate their input and queue one engine command. Failures always
//! use the envelope from [`error`]; an unknown path under the prefix is a JSON 404, not the SPA
//! shell.

mod blocking;
mod dispatch;
pub(crate) mod dto;
mod error;
mod extract;
mod health;
mod learning;
mod mode;
mod pack_alias;
mod pack_lookup;
mod pack_override;
mod packs;
mod params;
mod plan;
mod plans;
mod prices;
mod protection;
mod replan;
mod savings;
mod settings;
mod site;
mod stream;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, patch, post};

use crate::service::Service;

use error::ApiError;

/// An unknown `/api/v1/...` path.
async fn not_found() -> ApiError {
    ApiError::not_found("no such endpoint")
}

/// The `/api/v1` router (nested by [`crate::app::router`]).
pub fn router() -> Router<Arc<Service>> {
    Router::new()
        .route("/health", get(health::health))
        .route("/site", get(site::site))
        .route("/packs", get(packs::packs))
        .route("/packs/{sn}", patch(pack_alias::patch_alias))
        .route("/prices", get(prices::prices))
        .route("/plan/current", get(plan::current_plan))
        .route("/plans", get(plans::plans))
        .route("/dispatch", get(dispatch::dispatch))
        .route("/savings", get(savings::savings))
        .route(
            "/settings",
            get(settings::get_settings).patch(settings::patch_settings),
        )
        .route("/mode", patch(mode::patch_mode))
        .route("/override", post(pack_override::create_override))
        .route("/protection", post(protection::protection))
        .route("/learning", post(learning::learning))
        .route("/replan", post(replan::replan))
        .route("/stream", get(stream::stream))
        .fallback(not_found)
}
