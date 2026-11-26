//! Authentication middleware: validates tokens and injects auth context.

use salvo::http::{Request, StatusCode, StatusError};
use salvo::prelude::*;

use common::TokenScope;

use crate::auth::guard::bearer;
use crate::auth::jwt::{validate, AuthUser};
use crate::config::Config;

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
        match attempted {
            Ok(auth) => {
                depot.insert(AUTH_KEY, auth);
            }
            Err(e) => {
                ctrl.skip_rest();
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": e.brief,
                    "code": "UNAUTHORIZED",
                })));
            }
        }
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
