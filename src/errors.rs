use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("not found")]
    NotFound,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("validation failed")]
    Validation(#[from] validator::ValidationErrors),

    #[error("database error")]
    Database(#[from] sqlx::Error),

    #[error("internal error")]
    Internal(String),

    #[error(transparent)]
    Argon(#[from] bcrypt::BcryptError),

    #[error(transparent)]
    Jwt(#[from] jsonwebtoken::errors::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", self.to_string()),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
            AppError::Validation(errors) => {
                let details: Vec<String> = errors
                    .field_errors()
                    .into_iter()
                    .flat_map(|(field, errs)| {
                        errs.iter().map(move |e| {
                            let msg = e
                                .message
                                .as_ref()
                                .map(|m| m.to_string())
                                .unwrap_or_default();
                            format!("{}: {}", field, msg)
                        })
                    })
                    .collect();
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "validation_failed", "message": "validation failed", "details": details})),
                )
                    .into_response();
            }
            AppError::Database(sqlx::Error::RowNotFound) => (
                StatusCode::NOT_FOUND,
                "not_found",
                "resource not found".into(),
            ),
            AppError::Database(_) | AppError::Internal(_) => {
                tracing::error!("internal error: {:?}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".into(),
                )
            }
            AppError::Argon(_) | AppError::Jwt(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "internal server error".into(),
            ),
        };

        let mut response = Json(json!({"error": kind, "message": message})).into_response();
        if status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert("WWW-Authenticate", "Bearer".parse().unwrap());
        }
        *response.status_mut() = status;
        response
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
