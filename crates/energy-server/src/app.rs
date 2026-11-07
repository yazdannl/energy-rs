//! Router assembly.

use axum::Router;

use crate::{api, assets};

/// Build the application router: JSON API under `/api/v1`, embedded UI everywhere else.
pub fn router() -> Router {
    Router::new()
        .nest("/api/v1", api::router())
        .fallback(assets::serve)
}
