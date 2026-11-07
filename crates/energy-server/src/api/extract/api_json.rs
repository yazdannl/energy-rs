//! A JSON body extractor that fails with the API envelope instead of plain text.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use serde::de::DeserializeOwned;

use crate::api::error::ApiError;

/// A JSON body, or a `400 invalid_body` envelope.
///
/// The extractor message is deliberately generic: `serde_json` errors can quote the offending
/// value, and a request body may contain private settings. The status is kept in `details` for the
/// UI, nothing else.
pub(crate) struct ApiJson<T>(pub(crate) T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(reject(&rejection)),
        }
    }
}

fn reject(rejection: &JsonRejection) -> ApiError {
    tracing::debug!(status = %rejection.status(), "json body rejected");
    let status = rejection.status().as_u16();
    if status == 415 {
        return ApiError::body("the request body must be sent as application/json")
            .with_details(serde_json::json!({ "status": status }));
    }
    ApiError::body("the request body is not the documented JSON shape")
        .with_details(serde_json::json!({ "status": status }))
}
