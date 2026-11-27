//! JWT issuing and validation (HS256).
//!
//! Two token kinds share the same secret:
//! - `user` tokens: full API access, 24h (or 30d with stayLoggedIn)
//! - `attachment` tokens: media-only access, 7d

use chrono::{Duration, Utc};
use common::dto::Ts;
use common::{GroupRole, TokenScope};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

use crate::error::{AppError, AppResult};

/// Claims carried inside every token.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    /// Subject: user id.
    pub sub: String,
    /// Group (tenant) id.
    pub gid: String,
    /// Token scope.
    pub scope: String,
    /// Group role (user|owner).
    pub role: String,
    /// Password/token version — bumped on password change to revoke old tokens.
    pub tv: i64,
    /// Expiry (unix seconds).
    pub exp: i64,
    /// Issued at (unix seconds).
    pub iat: i64,
}

/// A validated token's payload.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub group_id: String,
    pub role: GroupRole,
    pub scope: TokenScope,
    pub expires_at: Ts,
    /// Token version captured at issue time.
    pub token_version: i64,
}

/// Lifetime for a normal user token.
pub const USER_TOKEN_HOURS: i64 = 24;
/// Lifetime when "stay logged in" was checked.
pub const USER_TOKEN_STAY_DAYS: i64 = 30;
/// Lifetime for media tokens used in `<img src>`.
pub const ATTACHMENT_TOKEN_DAYS: i64 = 7;

fn key(secret: &str) -> AppResult<EncodingKey> {
    EncodingKey::from_secret(secret.as_bytes())
        .map_err(|e| AppError::Internal(format!("jwt encoding key: {e}")))
}

fn decode_key(secret: &str) -> AppResult<DecodingKey> {
    DecodingKey::from_secret(secret.as_bytes())
        .map_err(|e| AppError::Internal(format!("jwt decoding key: {e}")))
}

/// Issue a token with the default (user) role.
///
/// # Errors
/// Fails when the clock or encoder rejects the claims.
pub fn issue(
    secret: &str,
    user_id: &str,
    group_id: &str,
    scope: TokenScope,
    stay_logged_in: bool,
) -> AppResult<(String, Ts)> {
    issue_with_role(secret, user_id, group_id, GroupRole::User, scope, stay_logged_in, 0)
}

/// Issue a token carrying the member's group role.
///
/// # Errors
/// Fails when the clock or encoder rejects the claims.
pub fn issue_with_role(
    secret: &str,
    user_id: &str,
    group_id: &str,
    role: GroupRole,
    scope: TokenScope,
    stay_logged_in: bool,
    token_version: i64,
) -> AppResult<(String, Ts)> {
    let now = Utc::now();
    let hours = match scope {
        TokenScope::User if stay_logged_in => USER_TOKEN_STAY_DAYS * 24,
        TokenScope::User => USER_TOKEN_HOURS,
        TokenScope::Attachments => ATTACHMENT_TOKEN_DAYS * 24,
    };
    let expires = now + Duration::hours(hours);
    let claims = Claims {
        sub: user_id.to_string(),
        gid: group_id.to_string(),
        scope: scope.to_string(),
        role: role.to_string(),
        tv: token_version,
        exp: expires.timestamp(),
        iat: now.timestamp(),
    };
    let token = encode(&Header::default(), &claims, &key(secret)?)
        .map_err(|e| AppError::Internal(format!("encode token: {e}")))?;
    Ok((token, expires))
}

/// Validate a token string.
///
/// # Errors
/// Returns `Unauthorized` for missing/invalid/expired tokens or wrong scope.
pub fn validate(secret: &str, token: &str, required: TokenScope) -> AppResult<AuthUser> {
    let data = decode::<Claims>(token, &decode_key(secret)?, &Validation::default())
        .map_err(|_| AppError::Unauthorized("无效令牌".into()))?;
    let claims = data.claims;
    let scope: TokenScope = claims
        .scope
        .parse()
        .map_err(|_| AppError::Unauthorized("令牌作用域无效".into()))?;
    if scope != required {
        return Err(AppError::Unauthorized("令牌作用域不匹配".into()));
    }
    let expires_at = chrono::DateTime::from_timestamp(claims.exp, 0)
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(|| AppError::Unauthorized("令牌过期".into()))?;
    let role: GroupRole = claims
        .role
        .parse()
        .map_err(|_| AppError::Unauthorized("令牌角色无效".into()))?;
    Ok(AuthUser {
        user_id: claims.sub,
        group_id: claims.gid,
        role,
        scope,
        expires_at,
        token_version: claims.tv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret";

    #[test]
    fn issue_and_validate_user_token() {
        let (token, exp) = issue(SECRET, "u1", "g1", TokenScope::User, false).unwrap();
        let auth = validate(SECRET, &token, TokenScope::User).unwrap();
        assert_eq!(auth.user_id, "u1");
        assert_eq!(auth.group_id, "g1");
        assert_eq!(auth.scope, TokenScope::User);
        let delta = exp - Utc::now();
        assert!(delta.num_hours() >= 23 && delta.num_hours() <= 24, "{delta}");
    }

    #[test]
    fn role_roundtrips() {
        let (token, _) = issue_with_role(SECRET, "u1", "g1", GroupRole::Owner, TokenScope::User, false, 0).unwrap();
        let auth = validate(SECRET, &token, TokenScope::User).unwrap();
        assert_eq!(auth.role, GroupRole::Owner);
    }

    #[test]
    fn stay_logged_in_extends_lifetime() {
        let (_, exp) = issue(SECRET, "u1", "g1", TokenScope::User, true).unwrap();
        let delta = exp - Utc::now();
        assert!(delta.num_days() >= 29, "{delta}");
    }

    #[test]
    fn scope_is_enforced() {
        let (token, _) = issue(SECRET, "u1", "g1", TokenScope::Attachments, false).unwrap();
        assert!(validate(SECRET, &token, TokenScope::User).is_err());
        let (utoken, _) = issue(SECRET, "u1", "g1", TokenScope::User, false).unwrap();
        assert!(validate(SECRET, &utoken, TokenScope::Attachments).is_err());
    }

    #[test]
    fn wrong_secret_rejected() {
        let (token, _) = issue(SECRET, "u1", "g1", TokenScope::User, false).unwrap();
        assert!(validate("other-secret", &token, TokenScope::User).is_err());
    }

    #[test]
    fn garbage_token_rejected() {
        assert!(validate(SECRET, "not.a.jwt", TokenScope::User).is_err());
        assert!(validate(SECRET, "", TokenScope::User).is_err());
    }
}
