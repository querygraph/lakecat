use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use lakecat_core::LakeCatError;
use serde_json::Value;

use crate::LakeCatHttpError;

#[tokio::test]
async fn unavailable_errors_use_retryable_iceberg_response() {
    let response = LakeCatHttpError(LakeCatError::Unavailable(
        "catalog storage is temporarily busy; retry the request".to_string(),
    ))
    .into_response();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["error"]["code"], 503);
    assert_eq!(payload["error"]["type"], "ServiceUnavailableException");
    assert_eq!(
        payload["error"]["message"],
        "temporarily unavailable: catalog storage is temporarily busy; retry the request"
    );
}
