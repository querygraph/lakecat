use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use lakecat_core::LakeCatError;
use serde_json::json;

#[derive(Debug)]
pub struct LakeCatHttpError(pub(crate) LakeCatError);

impl From<LakeCatError> for LakeCatHttpError {
    fn from(value: LakeCatError) -> Self {
        Self(value)
    }
}

impl IntoResponse for LakeCatHttpError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            LakeCatError::InvalidArgument(_) => StatusCode::BAD_REQUEST,
            LakeCatError::UnprocessableEntity(_) => StatusCode::UNPROCESSABLE_ENTITY,
            LakeCatError::NotFound { .. } => StatusCode::NOT_FOUND,
            LakeCatError::AlreadyExists { .. } | LakeCatError::Conflict(_) => StatusCode::CONFLICT,
            LakeCatError::Forbidden(_) => StatusCode::FORBIDDEN,
            LakeCatError::NotSupported(_) => StatusCode::NOT_IMPLEMENTED,
            LakeCatError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            LakeCatError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        // Iceberg REST ErrorModel `type` names, so stock clients (pyiceberg,
        // Spark, Trino) can map errors to their catalog exception types.
        let error_type = match &self.0 {
            LakeCatError::InvalidArgument(_) => "BadRequestException",
            LakeCatError::UnprocessableEntity(_) => "UnprocessableEntityException",
            LakeCatError::NotFound { object, .. } => match *object {
                "table" | "soft-deleted table" => "NoSuchTableException",
                "namespace" => "NoSuchNamespaceException",
                "view" => "NoSuchViewException",
                _ => "NotFoundException",
            },
            LakeCatError::AlreadyExists { .. } => "AlreadyExistsException",
            LakeCatError::Conflict(_) => "CommitFailedException",
            LakeCatError::Forbidden(_) => "ForbiddenException",
            LakeCatError::NotSupported(_) => "UnsupportedOperationException",
            LakeCatError::Unavailable(_) => "ServiceUnavailableException",
            LakeCatError::Internal(_) => "InternalServerError",
        };
        let body = Json(json!({
            "error": {
                "message": self.0.to_string(),
                "type": error_type,
                "code": status.as_u16()
            }
        }));
        (status, body).into_response()
    }
}
