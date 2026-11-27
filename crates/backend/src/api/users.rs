//! Auth + user management routes.

use salvo::writing::Json;

use common::dto::{
    ChangePasswordRequest, LoginForm, RegisterRequest, SetupRequest, TokenResponse, UserOut,
    UserUpdate,
};
use common::TokenScope;

use crate::auth::guard::{json_body, require_user};
use crate::auth::{jwt, password};
use crate::db::repo::users::{self, UserRecord};
use crate::db::{with_read, with_write};
use crate::error::{AppError, AppResult};
use crate::router;

fn build_token_response(
    config: &crate::config::Config,
    user: &UserRecord,
    stay: bool,
) -> AppResult<TokenResponse> {
    let (token, expires_at) = jwt::issue_with_role(
        &config.jwt_secret,
        &user.id,
        &user.group_id,
        user.role,
        TokenScope::User,
        stay,
        user.token_version,
    )?;
    let (attachment_token, _) = jwt::issue_with_role(
        &config.jwt_secret,
        &user.id,
        &user.group_id,
        user.role,
        TokenScope::Attachments,
        false,
        user.token_version,
    )?;
    Ok(TokenResponse {
        token,
        attachment_token,
        expires_at,
        user: user.to_out(),
    })
}

/// POST /api/v1/setup — first-run: create owner + group.
///
/// # Errors
/// `Conflict` when a user already exists.
#[salvo::handler]
pub async fn setup(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<TokenResponse>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let body: SetupRequest = json_body(req).await?;
    if with_read(pool, |conn| users::any_user_exists(conn)).await? {
        return Err(AppError::Conflict("系统已初始化，请直接登录".into()));
    }
    let hash = password::hash_password(&body.password)?;
    let name = body.name.clone();
    let email = body.email.clone();
    let out = with_write(pool, move |conn| users::setup_owner(conn, &name, &email, &hash)).await?;
    Ok(Json(build_token_response(&config, &out, true)?))
}

/// POST /api/v1/login.
///
/// # Errors
/// `Unauthorized` on bad credentials.
#[salvo::handler]
pub async fn login(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<TokenResponse>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let body: LoginForm = json_body(req).await?;
    let username = body.username.clone();
    let stored = with_read(pool, move |conn| users::find_by_login(conn, &username))
        .await?
        .ok_or_else(|| AppError::Unauthorized("用户名或密码错误".into()))?;
    if !password::verify_password(&body.password, stored.password_hash.as_deref().unwrap_or("")) {
        return Err(AppError::Unauthorized("用户名或密码错误".into()));
    }
    Ok(Json(build_token_response(&config, &stored, body.stay_logged_in)?))
}

/// GET /api/v1/user — current profile.
///
/// # Errors
/// `Unauthorized` / `NotFound`.
#[salvo::handler]
pub async fn me(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<UserOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let user_id = auth.user_id.clone();
    let user = with_read(pool, move |conn| users::find_by_id(conn, &user_id))
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".into()))?;
    Ok(Json(user.to_out()))
}

/// PUT /api/v1/user — update profile.
///
/// # Errors
/// `Validation`/`Conflict` per repo rules.
#[salvo::handler]
pub async fn update_me(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<UserOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let body: UserUpdate = json_body(req).await?;
    let user_id = auth.user_id.clone();
    let name = body.name.clone();
    let email = body.email.clone();
    let check_id = user_id.clone();
    if let Some(e) = email.clone() {
        if with_read(pool, move |conn| users::email_taken(conn, &e, Some(&check_id))).await? {
            return Err(AppError::Conflict("邮箱已被其他账号使用".into()));
        }
    }
    with_write(pool, move |conn| {
        users::update_profile(conn, &user_id, name.as_deref(), email.as_deref())
    })
    .await?;
    let user_id = auth.user_id.clone();
    let user = with_read(pool, move |conn| users::find_by_id(conn, &user_id))
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".into()))?;
    Ok(Json(user.to_out()))
}

/// PUT /api/v1/user/password.
///
/// # Errors
/// `Unauthorized` when the old password does not match.
#[salvo::handler]
pub async fn change_password(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let body: ChangePasswordRequest = json_body(req).await?;
    let user_id = auth.user_id.clone();
    let current = body.current.clone();
    let new_password = body.new.clone();
    with_write(pool, move |conn| {
        let user = users::find_by_id(conn, &user_id)?
            .ok_or_else(|| AppError::NotFound("用户不存在".into()))?;
        if !password::verify_password(&current, user.password_hash.as_deref().unwrap_or("")) {
            return Err(AppError::Unauthorized("当前密码不正确".into()));
        }
        let hash = password::hash_password(&new_password)?;
        users::set_password(conn, &user_id, &hash)
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /api/v1/register — registration via invitation token.
///
/// # Errors
/// `Forbidden` when registration is closed; `Conflict` on duplicate email.
#[salvo::handler]
pub async fn register(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<TokenResponse>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let body: RegisterRequest = json_body(req).await?;
    let (name, email, token, open) = (
        body.name.clone(),
        body.email.clone(),
        body.token.clone(),
        config.allow_registration,
    );
    // Validate the registration gate BEFORE spending CPU on Argon2 hashing:
    // otherwise a closed instance can be DoS'd with high-cost hash requests.
    let gate_token = token.clone();
    let gate_ok = with_read(pool, move |conn| {
        if gate_token.trim().is_empty() {
            Ok(open)
        } else {
            Ok(crate::db::repo::groups::find_invitation(conn, &gate_token)
                .map(|v| v.is_some())
                .unwrap_or(false))
        }
    })
    .await?;
    if !gate_ok {
        return Err(AppError::Forbidden("当前未开放注册，或邀请令牌无效".into()));
    }
    // Duplicate email must be reported before hashing too.
    let email2 = email.clone();
    if with_read(pool, move |conn| users::email_taken(conn, &email2, None)).await? {
        return Err(AppError::Conflict("邮箱已被注册".into()));
    }
    let hash = password::hash_password(&body.password)?;
    let out = with_write(pool, move |conn| {
        users::register_with_invitation(conn, open, &name, &email, &hash, &token)
    })
    .await?;
    Ok(Json(build_token_response(&config, &out, true)?))
}
