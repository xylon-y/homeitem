//! Application error type mapped to HTTP responses.

use common::error::ApiError;
use salvo::http::{Request, Response, StatusCode};
use salvo::prelude::*;

/// Errors surfaced by handlers and repositories.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    PayloadTooLarge(String),
    #[error("{0}")]
    Internal(String),
    #[error("{0}")]
    Database(#[from] rusqlite::Error),
    #[error("{0}")]
    Pool(#[from] r2d2::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    /// Stable machine-readable code for the wire format.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NOT_FOUND",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::Validation(_) => "VALIDATION",
            Self::Conflict(_) => "CONFLICT",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::PayloadTooLarge(_) => "PAYLOAD_TOO_LARGE",
            Self::Internal(_) => "INTERNAL",
            Self::Database(_) | Self::Pool(_) => "INTERNAL",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) | Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Internal(_) | Self::Database(_) | Self::Pool(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    /// Public payload: internal causes are never leaked to clients.
    fn payload(&self) -> ApiError {
        match self {
            Self::Internal(_) | Self::Database(_) | Self::Pool(_) => {
                ApiError::new(self.code(), "internal server error")
            }
            other => ApiError::new(other.code(), other.to_string()),
        }
    }
}

/// `AppError` writes itself as a JSON error body with the right status.
///
/// Combined with salvo's `Writer` impl for `Result<T, E>`, handlers can
/// simply return `AppResult<Json<T>>` and let errors render themselves.
#[async_trait::async_trait]
impl Writer for AppError {
    async fn write(self, _req: &mut Request, _dep: &mut Depot, res: &mut Response) {
        res.status_code(self.status());
        res.render(Json(self.payload()));
    }
}

/// Convenience: render an `AppError` into a response without early-returning.
pub fn render_error(err: &AppError, res: &mut Response) {
    res.status_code(err.status());
    res.render(Json(err.payload()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_errors_are_masked() {
        let e = AppError::Internal("db password leaked".into());
        let p = e.payload();
        assert_eq!(p.code, "INTERNAL");
        assert!(!p.error.contains("password"));
    }

    #[test]
    fn validation_keeps_message() {
        let e = AppError::Validation("name is required".into());
        assert_eq!(e.code(), "VALIDATION");
        assert_eq!(e.payload().error, "name is required");
    }
}
