//! Router assembly.

use std::sync::Arc;

use axum::Router;

use crate::{api, assets, service::Service};

/// Build the application router: JSON API under `/api/v1`, embedded UI everywhere else.
pub fn router(service: Arc<Service>) -> Router {
    Router::new()
        .nest("/api/v1", api::router())
        .fallback(assets::serve)
        .with_state(service)
}
