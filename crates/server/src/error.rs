//! API error type: every failure becomes a JSON `ApiError` with a matching status code.

use api_types::ApiError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use estimator::EstimateError;

#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub body: ApiError,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(status: StatusCode, code: &str, message: impl Into<String>) -> AppError {
        AppError {
            status,
            body: ApiError {
                code: code.into(),
                message: message.into(),
                estimate_error: None,
            },
        }
    }

    pub fn unauthorized() -> AppError {
        AppError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Sign in or provide a valid API key.",
        )
    }

    pub fn forbidden(message: impl Into<String>) -> AppError {
        AppError::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    pub fn not_found(what: &str) -> AppError {
        AppError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("{what} not found."),
        )
    }

    pub fn bad_request(message: impl Into<String>) -> AppError {
        AppError::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub fn conflict(message: impl Into<String>) -> AppError {
        AppError::new(StatusCode::CONFLICT, "conflict", message)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> AppError {
        tracing::error!(error = ?e, "internal error");
        AppError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Something went wrong. Try again.",
        )
    }
}

impl From<EstimateError> for AppError {
    fn from(e: EstimateError) -> AppError {
        AppError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            body: ApiError {
                code: "invalid_design".into(),
                message: e.message.clone(),
                estimate_error: Some(e),
            },
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}
