//! A query-string extractor that fails with the API envelope instead of plain text.

use axum::extract::rejection::QueryRejection;
use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::api::error::ApiError;

/// A deserialized query string, or a `400 invalid_query` envelope.
pub(crate) struct ApiQuery<T>(pub(crate) T);

impl<T, S> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(value)) => Ok(Self(value)),
            Err(rejection) => Err(reject(&rejection)),
        }
    }
}

fn reject(rejection: &QueryRejection) -> ApiError {
    tracing::debug!(status = %rejection.status(), "query string rejected");
    ApiError::query("the query string is not valid for this endpoint")
        .with_details(serde_json::json!({ "status": rejection.status().as_u16() }))
}
