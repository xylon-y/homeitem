//! Item/location CRUD, filtering, tree, statistics and reminder routes.

use salvo::writing::Json;

use common::dto::{
    DuplicateOptions, EntityCreate, EntityOut, EntityPatch, EntityPath, EntitySummary,
    EntityUpdate, Page, ReminderResponse, Statistics, TotalsByOrganizer, TreeItem, ValueOverTimeEntry,
};
use common::query::{EntitiesQuery, RemindersQuery};

use crate::auth::guard::{json_body, require_user};
use crate::db::repo::{entities as repo, notifiers, taxonomy};
use crate::db::{with_read, with_write};
use crate::error::AppResult;
use crate::router;

/// Parse `EntitiesQuery` from the request query string.
///
/// Array-ish fields accept a single comma-separated value. Unknown keys are
/// ignored; numeric garbage falls back to defaults.
pub fn parse_entities_query(req: &salvo::Request) -> EntitiesQuery {
    let queries = req.queries();
    let one = |key: &str| queries.get(key).cloned().unwrap_or_default();
    let flag = |key: &str| matches!(one(key).to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
    EntitiesQuery {
        q: one("q"),
        page: one("page").parse().unwrap_or(0),
        page_size: one("pageSize").parse().unwrap_or(0),
        order_by: one("orderBy"),
        order_direction: one("orderDirection"),
        include_archived: flag("includeArchived"),
        parent_ids: queries.get("parentIds").cloned().filter(|s| !s.is_empty()),
        entity_type_ids: queries.get("entityTypeIds").cloned().filter(|s| !s.is_empty()),
        tags: queries.get("tags").cloned().filter(|s| !s.is_empty()),
        negate_tags: flag("negateTags"),
        match_all_tags: flag("matchAllTags"),
        is_location: one("isLocation").parse::<i8>().ok().map(|v| v != 0),
        only_with_photo: flag("onlyWithPhoto"),
        only_without_photo: flag("onlyWithoutPhoto"),
        low_stock: flag("lowStock"),
        fields: queries.get("fields").cloned().filter(|s| !s.is_empty()),
    }
}

/// Parse `RemindersQuery` from the request query string.
pub fn parse_reminders_query(req: &salvo::Request) -> RemindersQuery {
    let queries = req.queries();
    let one = |key: &str| queries.get(key).cloned().unwrap_or_default();
    let flag = |key: &str| matches!(one(key).to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
    RemindersQuery {
        within: one("within").parse().unwrap_or(30),
        include_overdue: flag("includeOverdue"),
        include_done: flag("includeDone"),
        kinds: queries.get("kinds").cloned().filter(|s| !s.is_empty()),
    }
}

/// GET /api/v1/entities — filtered page.
///
/// # Errors
/// Database/query errors propagate; bad filters are clamped, not rejected.
#[salvo::handler]
pub async fn list_entities(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<Page<EntitySummary>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let query = parse_entities_query(req);
    let gid = auth.group_id.clone();
    let page = with_read(pool, move |conn| repo::list(conn, &gid, &query)).await?;
    Ok(Json(page))
}

/// GET /api/v1/entities/{id} — full detail.
///
/// # Errors
/// `NotFound` for other groups' items.
#[salvo::handler]
pub async fn get_entity(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<EntityOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let gid = auth.group_id.clone();
    let out = with_read(pool, move |conn| repo::get(conn, &gid, &id)).await?;
    Ok(Json(out))
}

/// POST /api/v1/entities — create item or location.
///
/// # Errors
/// `Validation` for bad references.
#[salvo::handler]
pub async fn create_entity(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<EntityOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let body: EntityCreate = json_body(req).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let gid = auth.group_id.clone();
    let out = with_write(pool, move |conn| {
        repo::validate_refs(conn, &gid, &body.entity_type_id, &body.parent_id, &body.location_id, &body.tag_ids)?;
        repo::create(conn, &gid, &id, &body)?;
        repo::get(conn, &gid, &id)
    })
    .await?;
    Ok(Json(out))
}

/// PUT /api/v1/entities/{id} — full update.
///
/// # Errors
/// `NotFound`/`Validation`.
#[salvo::handler]
pub async fn update_entity(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<EntityOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let body: EntityUpdate = json_body(req).await?;
    let gid = auth.group_id.clone();
    let out = with_write(pool, move |conn| {
        repo::validate_refs(conn, &gid, &body.entity_type_id, &body.parent_id, &body.location_id, &body.tag_ids)?;
        repo::update(conn, &gid, &id, &body)?;
        repo::get(conn, &gid, &id)
    })
    .await?;
    Ok(Json(out))
}

/// PATCH /api/v1/entities/{id} — partial update (quantity/archive/move).
///
/// # Errors
/// `NotFound`/`Validation`.
#[salvo::handler]
pub async fn patch_entity(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<EntityOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let body: EntityPatch = json_body(req).await?;
    let gid = auth.group_id.clone();
    let out = with_write(pool, move |conn| {
        repo::validate_patch_refs(conn, &gid, &body)?;
        repo::patch(conn, &gid, &id, &body)?;
        repo::get(conn, &gid, &id)
    })
    .await?;
    Ok(Json(out))
}

/// DELETE /api/v1/entities/{id} — removes row + attachment files from disk.
///
/// # Errors
/// `NotFound` when absent.
#[salvo::handler]
pub async fn delete_entity(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let gid = auth.group_id.clone();
    let files = with_write(pool, move |conn| repo::delete(conn, &gid, &id)).await?;
    for rel in files {
        let full = std::path::Path::new(&config.upload_dir).join(&rel);
        if full.exists() {
            let _ = std::fs::remove_file(&full);
        }
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /api/v1/entities/{id}/duplicate — copy item (+ children/options).
///
/// # Errors
/// `NotFound`/`Internal` for copy failures.
#[salvo::handler]
pub async fn duplicate_entity(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<EntityOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let body: DuplicateOptions = json_body(req).await?;
    let new_id = uuid::Uuid::new_v4().to_string();
    let gid = auth.group_id.clone();
    let out = with_write(pool, move |conn| {
        let copied = repo::duplicate(conn, &gid, &id, &new_id, &body)?;
        repo::get(conn, &gid, &new_id).map(|o| (o, copied))
    })
    .await?;
    let (out, copied) = out;
    // The repo stores new attachment paths under the new entity id; actually
    // copy the bytes so the duplicate's attachments resolve on disk.
    for (old_rel, new_rel) in copied {
        let src = config.upload_dir.join(&old_rel);
        let dst = config.upload_dir.join(&new_rel);
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(&src, &dst) {
            tracing::warn!(%e, ?src, ?dst, "failed to copy attachment on duplicate");
        }
    }
    Ok(Json(out))
}

/// GET /api/v1/entities/{id}/path — breadcrumb.
///
/// # Errors
/// `NotFound`.
#[salvo::handler]
pub async fn entity_path(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<Vec<EntityPath>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let gid = auth.group_id.clone();
    let p = with_read(pool, move |conn| repo::path(conn, &gid, &id)).await?;
    Ok(Json(p))
}

/// GET /api/v1/locations/tree — nested location tree with counts.
///
/// # Errors
/// Database errors propagate.
#[salvo::handler]
pub async fn location_tree(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<Vec<TreeItem>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let t = with_read(pool, move |conn| repo::tree(conn, &gid)).await?;
    Ok(Json(t))
}

/// GET /api/v1/entity-types — list (optionally only locations).
///
/// # Errors
/// Database errors propagate.
#[salvo::handler]
pub async fn entity_types(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<Vec<common::dto::EntityTypeOut>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let only_locations: Option<bool> = req.query::<bool>("locations");
    let gid = auth.group_id.clone();
    let types = with_read(pool, move |conn| taxonomy::list_types(conn, &gid, only_locations)).await?;
    Ok(Json(types))
}

/// GET /api/v1/tags — list with usage counts.
///
/// # Errors
/// Database errors propagate.
#[salvo::handler]
pub async fn list_tags(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<Vec<common::dto::TagSummary>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let tags_list = with_read(pool, move |conn| taxonomy::list_tags(conn, &gid, true)).await?;
    Ok(Json(tags_list))
}

/// GET /api/v1/reminders — due/overdue reminders across kinds.
///
/// # Errors
/// Database errors propagate.
#[salvo::handler]
pub async fn list_reminders(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<ReminderResponse>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let q = parse_reminders_query(req);
    let gid = auth.group_id.clone();
    let r = with_read(pool, move |conn| notifiers::reminders(conn, &gid, &q)).await?;
    Ok(Json(r))
}

/// GET /api/v1/statistics — headline numbers.
///
/// # Errors
/// Database errors propagate.
#[salvo::handler]
pub async fn get_statistics(depot: &mut salvo::Depot, req: &mut salvo::Request) -> AppResult<Json<Statistics>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let s = with_read(pool, move |conn| notifiers::statistics(conn, &gid)).await?;
    Ok(Json(s))
}

/// GET /api/v1/statistics/by-location.
///
/// # Errors
/// Database errors propagate.
#[salvo::handler]
pub async fn stats_by_location(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<Vec<TotalsByOrganizer>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let s = with_read(pool, move |conn| notifiers::stats_by_location(conn, &gid)).await?;
    Ok(Json(s))
}

/// GET /api/v1/statistics/by-tag.
///
/// # Errors
/// Database errors propagate.
#[salvo::handler]
pub async fn stats_by_tag(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<Vec<TotalsByOrganizer>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let s = with_read(pool, move |conn| notifiers::stats_by_tag(conn, &gid)).await?;
    Ok(Json(s))
}

/// GET /api/v1/statistics/value-over-time.
///
/// # Errors
/// Database errors propagate.
#[salvo::handler]
pub async fn stats_value_over_time(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<Vec<ValueOverTimeEntry>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let s = with_read(pool, move |conn| notifiers::stats_purchase_over_time(conn, &gid)).await?;
    Ok(Json(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_query_defaults() {
        // parse from an empty query map is exercised at runtime; here the
        // struct defaults are validated through the clamp helpers
        let q = EntitiesQuery::default();
        assert_eq!(q.page(), 1);
        assert_eq!(q.page_size(), 25);
    }
}
