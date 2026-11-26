//! Group management routes (multi-tenant administration).

use salvo::writing::Json;

use common::dto::{GroupOut, GroupUpdate, InvitationCreate, InvitationOut, MemberOut};
use common::GroupRole;

use crate::auth::guard::{json_body, require_user};
use crate::auth::jwt::AuthUser;
use crate::db::repo::groups;
use crate::db::repo::users;
use crate::db::{with_read, with_write};
use crate::error::{AppError, AppResult};
use crate::router;

/// Owners manage the group; plain members cannot.
fn require_owner(auth: &AuthUser) -> AppResult<()> {
    if auth.role == GroupRole::Owner {
        Ok(())
    } else {
        Err(AppError::Forbidden("需要组管理员权限".into()))
    }
}

/// GET /api/v1/group.
///
/// # Errors
/// `Unauthorized`/`NotFound`.
#[salvo::handler]
pub async fn get_group(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<GroupOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let g = with_read(pool, move |conn| groups::find(conn, &gid)).await?;
    Ok(Json(g))
}

/// PUT /api/v1/group — owner only.
///
/// # Errors
/// `Forbidden` for non-owners.
#[salvo::handler]
pub async fn update_group(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<GroupOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    require_owner(&auth)?;
    let body: GroupUpdate = json_body(req).await?;
    let gid = auth.group_id.clone();
    let (name, currency) = (body.name.clone(), body.currency.clone());
    let gid2 = gid.clone();
    with_write(pool, move |conn| groups::update(conn, &gid, &name, &currency)).await?;
    let g = with_read(pool, move |conn| groups::find(conn, &gid2)).await?;
    Ok(Json(g))
}

/// GET /api/v1/group/members.
///
/// # Errors
/// Database errors propagate.
#[salvo::handler]
pub async fn members(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<Vec<MemberOut>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let list = with_read(pool, move |conn| users::list_members_out(conn, &gid)).await?;
    Ok(Json(list))
}

/// DELETE /api/v1/group/members/{id} — owner only; cannot remove self or last member.
///
/// # Errors
/// `Forbidden`/`Conflict`/`NotFound`.
#[salvo::handler]
pub async fn remove_member(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    require_owner(&auth)?;
    let member_id = req.param::<String>("id").unwrap_or_default();
    if member_id == auth.user_id {
        return Err(AppError::Conflict("不能移除自己".into()));
    }
    let gid = auth.group_id.clone();
    let mid = member_id.clone();
    with_write(pool, move |conn| {
        let remaining = users::count_group_users(conn, &gid)?;
        if remaining <= 1 {
            return Err(AppError::Conflict("至少保留一名成员".into()));
        }
        users::remove_member(conn, &gid, &mid)
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// PATCH /api/v1/group/members/{id}/role — owner only.
///
/// # Errors
/// `Forbidden`/`Validation`.
#[salvo::handler]
pub async fn set_member_role(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    require_owner(&auth)?;
    let member_id = req.param::<String>("id").unwrap_or_default();
    let body: serde_json::Value = json_body(req).await?;
    let role_raw = body
        .get("role")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("缺少 role 字段".into()))?;
    let role: GroupRole = role_raw
        .parse()
        .map_err(|_| AppError::Validation("角色必须是 user 或 owner".into()))?;
    let gid = auth.group_id.clone();
    let mid = member_id.clone();
    with_write(pool, move |conn| {
        if role == GroupRole::User {
            let owners = users::count_owners(conn, &gid)?;
            let target_is_owner = users::member_role(conn, &gid, &mid)? == Some(GroupRole::Owner);
            if owners <= 1 && target_is_owner {
                return Err(AppError::Conflict("至少保留一名管理员".into()));
            }
        }
        users::set_role(conn, &mid, &gid, role)
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// GET /api/v1/group/invitations — owner only.
///
/// # Errors
/// `Forbidden`.
#[salvo::handler]
pub async fn invitations(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<Vec<InvitationOut>>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    require_owner(&auth)?;
    let gid = auth.group_id.clone();
    let list = with_read(pool, move |conn| groups::list_invitations(conn, &gid)).await?;
    Ok(Json(list))
}

/// POST /api/v1/group/invitations — owner only.
///
/// # Errors
/// `Forbidden`.
#[salvo::handler]
pub async fn create_invitation(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<InvitationOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    require_owner(&auth)?;
    let body: InvitationCreate = json_body(req).await?;
    let inv_id = uuid::Uuid::new_v4().to_string();
    let raw_token = uuid::Uuid::new_v4().simple().to_string();
    let gid = auth.group_id.clone();
    let (role, email) = (body.role, body.email.clone());
    let (inv_for_task, inv_for_lookup, gid2) = (inv_id.clone(), inv_id.clone(), gid.clone());
    with_write(pool, move |conn| {
        groups::create_invitation(conn, &inv_for_task, &gid, &raw_token, role, email.as_deref(), None)
    })
    .await?;
    let list = with_read(pool, move |conn| groups::list_invitations(conn, &gid2)).await?;
    let inv = list
        .into_iter()
        .find(|i| i.id == inv_for_lookup)
        .ok_or_else(|| AppError::Internal("invitation vanished".into()))?;
    Ok(Json(inv))
}

/// DELETE /api/v1/group/invitations/{id} — owner only.
///
/// # Errors
/// `Forbidden`/`NotFound`.
#[salvo::handler]
pub async fn delete_invitation(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    require_owner(&auth)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let gid = auth.group_id.clone();
    let n = with_write(pool, move |conn| groups::delete_invitation(conn, &gid, &id)).await?;
    if !n {
        return Err(AppError::NotFound("邀请不存在".into()));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use common::GroupRole;

    #[test]
    fn role_parse_rejects_unknown() {
        assert!("manager".parse::<GroupRole>().is_err());
        assert!("owner".parse::<GroupRole>().is_ok());
    }
}
