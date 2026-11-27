//! Tag / entity-type CRUD routes (read handlers live in `entities.rs`).

use salvo::writing::Json;

use common::dto::{
    EntityTypeCreate, EntityTypeOut, EntityTypeUpdate, TagCreate, TagOut, TagSummary, TagUpdate,
};

use crate::auth::guard::{json_body, require_user};
use crate::db::repo::taxonomy as repo;
use crate::db::{with_read, with_write};
use crate::error::{AppError, AppResult};
use crate::router;

// -- tags ------------------------------------------------------------------

/// GET /api/v1/tags/tree — nested tag tree with item counts.
#[salvo::handler]
pub async fn tag_tree(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<Vec<TagOut>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let tags = with_read(pool, move |conn| repo::list_tags(conn, &gid, true)).await?;
    Ok(Json(repo::tag_tree(tags, true)))
}

/// POST /api/v1/tags.
#[salvo::handler]
pub async fn create_tag(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<TagSummary>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let body: TagCreate = json_body(req).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let gid = auth.group_id.clone();
    let (gid_w, id_w, body_w) = (gid.clone(), id.clone(), body);
    with_write(pool, move |conn| repo::create_tag(conn, &gid_w, &id_w, &body_w)).await?;
    let tag = with_read(pool, move |conn| repo::find_tag(conn, &gid, &id)).await?;
    tag.ok_or_else(|| AppError::Internal("tag vanished".into())).map(Json)
}

/// PUT /api/v1/tags/{id}.
#[salvo::handler]
pub async fn update_tag(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<TagSummary>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let body: TagUpdate = json_body(req).await?;
    let gid = auth.group_id.clone();
    let (gid_w, id_w) = (gid.clone(), id.clone());
    with_write(pool, move |conn| repo::update_tag(conn, &gid_w, &id_w, &body)).await?;
    let tag = with_read(pool, move |conn| repo::find_tag(conn, &gid, &id)).await?;
    tag.ok_or_else(|| AppError::NotFound("标签不存在".into())).map(Json)
}

/// DELETE /api/v1/tags/{id}.
#[salvo::handler]
pub async fn delete_tag(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let gid = auth.group_id.clone();
    with_write(pool, move |conn| repo::delete_tag(conn, &gid, &id)).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// -- entity types ----------------------------------------------------------

/// POST /api/v1/entity-types.
#[salvo::handler]
pub async fn create_type(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<EntityTypeOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let body: EntityTypeCreate = json_body(req).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let gid = auth.group_id.clone();
    let (gid_w, id_w) = (gid.clone(), id.clone());
    with_write(pool, move |conn| repo::create_type(conn, &gid_w, &id_w, &body)).await?;
    let out = with_read(pool, move |conn| repo::find_type(conn, &gid, &id)).await?;
    out.ok_or_else(|| AppError::Internal("entity type vanished".into())).map(Json)
}

/// PUT /api/v1/entity-types/{id}.
#[salvo::handler]
pub async fn update_type(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<EntityTypeOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let body: EntityTypeUpdate = json_body(req).await?;
    let gid = auth.group_id.clone();
    let (gid_w, id_w) = (gid.clone(), id.clone());
    with_write(pool, move |conn| repo::update_type(conn, &gid_w, &id_w, &body)).await?;
    let out = with_read(pool, move |conn| repo::find_type(conn, &gid, &id)).await?;
    out.ok_or_else(|| AppError::NotFound("类型不存在".into())).map(Json)
}

/// DELETE /api/v1/entity-types/{id}.
#[salvo::handler]
pub async fn delete_type(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let gid = auth.group_id.clone();
    with_write(pool, move |conn| repo::delete_type(conn, &gid, &id)).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
