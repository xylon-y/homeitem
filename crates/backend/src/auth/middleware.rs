//! Authentication middleware: validates tokens and injects auth context.

use salvo::http::{Request, StatusCode, StatusError};
use salvo::prelude::*;

use common::TokenScope;

use crate::auth::guard::bearer;
use crate::auth::jwt::{validate, AuthUser};
use crate::config::Config;
use crate::db::with_read;

/// Depot key under which the validated user is stored.
pub const AUTH_KEY: &str = "homeitem.auth";

/// Validate `Authorization: Bearer` on every API request under its mount.
#[derive(Clone)]
pub struct AuthMiddleware {
    config: Config,
}

impl AuthMiddleware {
    /// Create middleware from config.
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Handler for AuthMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let attempted = bearer(req)
            .ok_or_else(|| StatusError::unauthorized().brief("缺少访问令牌"))
            .and_then(|t| {
                validate(&self.config.jwt_secret, &t, TokenScope::User)
                    .map_err(|_| StatusError::unauthorized().brief("无效令牌"))
            });
        let auth = match attempted {
            Ok(auth) => auth,
            Err(e) => {
                ctrl.skip_rest();
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": e.brief,
                    "code": "UNAUTHORIZED",
                })));
                return;
            }
        };

        // JWT is stateless: without a DB check, a removed member's or a
        // demoted owner's old token stays valid for up to 30 days. Verify
        // current membership and role on every request; reject stale tokens
        // so the user must re-authenticate and receive fresh claims.
        let pool = crate::router::pool(depot).clone();
        let uid = auth.user_id.clone();
        let gid = auth.group_id.clone();
        let claimed_role = auth.role.clone();
        let claimed_tv = auth.token_version;
        let live = with_read(&pool, move |conn| {
            let role = crate::db::repo::users::member_role(conn, &gid, &uid)?;
            let tv = crate::db::repo::users::token_version(conn, &uid)?;
            Ok((role, tv))
        })
        .await;

        let valid = match live {
            Ok((Some(role), tv)) => role == claimed_role && tv == claimed_tv,
            _ => false,
        };
        if !valid {
            ctrl.skip_rest();
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(salvo::writing::Json(serde_json::json!({
                "error": "成员资格或角色已变更，请重新登录",
                "code": "UNAUTHORIZED",
            })));
            return;
        }

        depot.insert(AUTH_KEY, auth);
    }
}

/// Fetch the injected auth user (set by [`AuthMiddleware`]).
///
/// # Panics
/// Only when a handler outside the guarded router calls it - a programming
/// error, not a runtime condition.
pub fn current_auth(depot: &Depot) -> &AuthUser {
    depot.obtain::<AuthUser>().expect("auth middleware must run first")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_key_is_stable() {
        assert_eq!(AUTH_KEY, "homeitem.auth");
    }
}
