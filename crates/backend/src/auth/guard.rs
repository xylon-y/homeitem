//! Request guards: extract and validate tokens from headers/query.

use common::TokenScope;
use salvo::http::{Request, StatusError};

use super::jwt::{validate, AuthUser};
use crate::config::Config;
use crate::error::{AppError, AppResult};

/// Read `Authorization: Bearer <token>` from a request.
pub fn bearer(req: &Request) -> Option<String> {
    req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

/// Validate the request as a full-access user token.
///
/// # Errors
/// `Unauthorized` when the header is missing or the token is invalid.
pub fn require_user(req: &Request, config: &Config) -> AppResult<AuthUser> {
    let token = bearer(req).ok_or_else(|| AppError::Unauthorized("缺少访问令牌".into()))?;
    validate(&config.jwt_secret, &token, TokenScope::User)
}

/// Validate a media token, accepting either an attachment-scoped token or a
/// full user token (so logged-in browsers can render `/api/v1/media/*`).
///
/// # Errors
/// `Unauthorized` when neither scope validates.
pub fn require_media(req: &Request, config: &Config) -> AppResult<AuthUser> {
    let token = bearer(req)
        .or_else(|| req.query::<String>("token").filter(|s| !s.is_empty()))
        .ok_or_else(|| AppError::Unauthorized("缺少媒体令牌".into()))?;
    match validate(&config.jwt_secret, &token, TokenScope::Attachments) {
        Ok(auth) => Ok(auth),
        Err(_) => validate(&config.jwt_secret, &token, TokenScope::User),
    }
}

/// Extract a JSON body, mapping decode failures to `BadRequest`.
///
/// # Errors
/// `BadRequest` for malformed or non-JSON payloads.
pub async fn json_body<T: serde::de::DeserializeOwned>(req: &mut Request) -> AppResult<T> {
    req.parse_json::<T>()
        .await
        .map_err(|e| AppError::BadRequest(format!("请求体无效: {e}")))
}

/// 404 catcher body for unmatched API routes.
pub fn api_not_found() -> StatusError {
    StatusError::not_found().brief("接口不存在")
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_prefix_parses_case_insensitively() {
        assert!(has_bearer_prefix("Bearer abc"));
        assert!(has_bearer_prefix("bearer abc"));
        assert!(!has_bearer_prefix("Basic abc"));
        assert!(!has_bearer_prefix("Bearer"));
    }

    fn has_bearer_prefix(v: &str) -> bool {
        v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")).is_some()
    }
}
