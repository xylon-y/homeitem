//! Maintenance-entry CRUD routes.

use salvo::writing::Json;

use common::dto::{MaintenanceCreate, MaintenanceEntry, MaintenanceUpdate};

use crate::auth::guard::{json_body, require_user};
use crate::db::repo::{entities, maintenance as repo};
use crate::db::{with_read, with_write};
use crate::error::{AppError, AppResult};
use crate::router;

/// Verify the entity belongs to the caller's group.
async fn ensure_owned(pool: &crate::db::Pool, gid: &str, eid: &str) -> AppResult<()> {
    let (g, e) = (gid.to_string(), eid.to_string());
    let ok = with_read(pool, move |conn| entities::entity_exists(conn, &g, &e)).await?;
    if !ok {
        return Err(AppError::NotFound("物品不存在".into()));
    }
    Ok(())
}

/// GET /api/v1/entities/{id}/maintenance?status=all|scheduled|completed
#[salvo::handler]
pub async fn list(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<Vec<MaintenanceEntry>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let eid = req.param::<String>("id").unwrap_or_default();
    ensure_owned(pool, &auth.group_id, &eid).await?;
    let status = req.query::<String>("status").unwrap_or_else(|| "all".into());
    let out = with_read(pool, move |conn| repo::list(conn, &eid, &status)).await?;
    Ok(Json(out))
}

/// POST /api/v1/entities/{id}/maintenance.
#[salvo::handler]
pub async fn create(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<MaintenanceEntry>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let eid = req.param::<String>("id").unwrap_or_default();
    ensure_owned(pool, &auth.group_id, &eid).await?;
    let body: MaintenanceCreate = json_body(req).await?;
    let out = with_write(pool, move |conn| {
        let id = repo::create(conn, &eid, &body)?;
        repo::find(conn, &eid, &id)?.ok_or_else(|| AppError::Internal("maintenance vanished".into()))
    })
    .await?;
    Ok(Json(out))
}

/// PUT /api/v1/entities/{id}/maintenance/{mid}.
#[salvo::handler]
pub async fn update(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<MaintenanceEntry>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let eid = req.param::<String>("id").unwrap_or_default();
    ensure_owned(pool, &auth.group_id, &eid).await?;
    let mid = req.param::<String>("mid").unwrap_or_default();
    let body: MaintenanceUpdate = json_body(req).await?;
    let out = with_write(pool, move |conn| {
        repo::update(conn, &eid, &mid, &body)?;
        repo::find(conn, &eid, &mid)?.ok_or_else(|| AppError::NotFound("保养记录不存在".into()))
    })
    .await?;
    Ok(Json(out))
}

/// DELETE /api/v1/entities/{id}/maintenance/{mid}.
#[salvo::handler]
pub async fn delete(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let eid = req.param::<String>("id").unwrap_or_default();
    ensure_owned(pool, &auth.group_id, &eid).await?;
    let mid = req.param::<String>("mid").unwrap_or_default();
    with_write(pool, move |conn| repo::delete(conn, &eid, &mid)).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
