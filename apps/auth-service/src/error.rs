use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    NotFound,
    Unauthorized,
    Forbidden,
    BadRequest(String),
    Conflict(String),
    Internal,
}

pub type Result<T> = std::result::Result<T, AppError>;

/// Logs the cause and hides it from the client.
pub fn internal(e: impl std::fmt::Display) -> AppError {
    tracing::error!("internal error: {e}");
    AppError::Internal
}

pub fn bad(msg: &str) -> AppError {
    AppError::BadRequest(msg.into())
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found".into()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden".into()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, m),
            AppError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            sqlx::Error::Database(d) if d.is_unique_violation() => AppError::Conflict("already exists".into()),
            sqlx::Error::Database(d) if d.is_check_violation() || d.is_foreign_key_violation() => bad("invalid value"),
            _ => internal(e),
        }
    }
}
