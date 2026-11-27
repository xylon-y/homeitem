//! 附件上传（multipart）、下载与删除路由。

use salvo::writing::Json;

use common::dto::ItemAttachment;

use crate::auth::guard::require_user;
use crate::auth::jwt::AuthUser;
use crate::config::Config;
use crate::db::repo::attachments as repo;
use crate::db::{with_read, with_write};
use crate::error::{AppError, AppResult};
use crate::router;

/// POST /api/v1/entities/{id}/attachments — multipart upload.
///
/// Fields: `file` (binary), `type` (photo|manual|warranty|attachment|receipt),
/// `title` (optional text).
///
/// # Errors
/// `NotFound` when the entity is missing; `PayloadTooLarge` over the limit.
#[salvo::handler]
pub async fn upload(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
    res: &mut salvo::Response,
) -> Result<(), crate::error::AppError> {
    let pool = router::pool(depot).clone();
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let entity_id = req.param::<String>("id").unwrap_or_default();

    // Verify entity ownership BEFORE touching the filesystem, so a foreign or
    // non-existent entity cannot leave orphan files or create out-of-tree dirs.
    let (gid, eid) = (auth.group_id.clone(), entity_id.clone());
    let owned = with_read(&pool, move |conn| {
        crate::db::repo::entities::entity_exists(conn, &gid, &eid)
    })
    .await?;
    if !owned {
        return Err(AppError::NotFound("物品不存在".into()));
    }

    let form = req
        .form_data()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart 解析失败: {e}")))?;
    let max_bytes = (config.max_upload_mb as usize) * 1024 * 1024;

    let file = form
        .files
        .get("file")
        .cloned()
        .ok_or_else(|| AppError::BadRequest("缺少 file 字段".into()))?;
    if file.size() > max_bytes as u64 {
        return Err(AppError::PayloadTooLarge(format!(
            "文件超过 {} MB 上限",
            config.max_upload_mb
        )));
    }
    let kind = form
        .fields
        .get("type")
        .cloned()
        .unwrap_or_else(|| "attachment".into());
    let title = form.fields.get("title").cloned().unwrap_or_default();
    let mime = file
        .content_type()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".into());
    let original_name = file.name().unwrap_or("upload.bin").to_string();

    // move the temp file into our upload dir: {entity_id}/{attachment_id}{ext}
    let attachment_id = uuid::Uuid::new_v4().to_string();
    let ext = std::path::Path::new(&original_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let rel = format!("{entity_id}/{attachment_id}{ext}");

    // create the entity's upload dir first so safe_join can canonicalize it
    let base = config.upload_dir.join(&entity_id);
    std::fs::create_dir_all(&base)
        .map_err(|e| AppError::Internal(format!("create upload dir: {e}")))?;
    let dst = crate::db::safe_join(&config.upload_dir, &rel)?;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(format!("create upload dir: {e}")))?;
    }
    let is_primary = kind == "photo";
    let size = file.size() as i64;
    std::fs::rename(file.path(), &dst)
        .or_else(|_| std::fs::copy(file.path(), &dst).map(|_| ()))
        .map_err(|e| AppError::Internal(format!("save upload: {e}")))?;

    let (eid, aid, rel2) = (entity_id.clone(), attachment_id.clone(), rel.clone());
    let (kind2, title2, mime2) = (kind.clone(), title.clone(), mime.clone());
    let out = with_write(&pool, move |conn| {
        repo::insert(conn, &aid, &eid, &kind2, &title2, &original_name, &rel2, &mime2, size, is_primary)?;
        repo::find(conn, &eid, &aid)?.ok_or_else(|| AppError::Internal("attachment vanished".into()))
    })
    .await?;
    res.render(Json(out));
    Ok(())
}

/// GET /api/v1/entities/{id}/attachments — list.
///
/// # Errors
/// `Unauthorized`/`NotFound`.
#[salvo::handler]
pub async fn list(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<Vec<ItemAttachment>>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let entity_id = req.param::<String>("id").unwrap_or_default();
    let eid = entity_id.clone();
    let exists = with_read(pool, move |conn| {
        crate::db::repo::entities::entity_exists(conn, &auth.group_id, &eid)
    })
    .await?;
    if !exists {
        return Err(AppError::NotFound("物品不存在".into()));
    }
    let list = with_read(pool, move |conn| repo::list(conn, &entity_id)).await?;
    Ok(Json(list))
}

/// GET /api/v1/entities/{id}/attachments/{aid} — download bytes.
///
/// Accepts either a user token or an attachment token (media embedding).
///
/// # Errors
/// `NotFound`/`Unauthorized`.
#[salvo::handler]
pub async fn download(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
    res: &mut salvo::Response,
) -> Result<(), crate::error::AppError> {
    let pool = router::pool(depot).clone();
    let config = router::config(depot).clone();
    let auth = require_media(&config, req)?;
    let entity_id = req.param::<String>("id").unwrap_or_default();
    let attachment_id = req.param::<String>("aid").unwrap_or_default();

    // Ownership check applies to BOTH token scopes. Attachment (media) tokens
    // carry a signed `gid`; skipping this check allowed cross-tenant IDOR.
    let (gid, eid2, aid2) = (auth.group_id.clone(), entity_id.clone(), attachment_id.clone());
    let att = with_read(&pool, move |conn| {
        if !crate::db::repo::entities::entity_exists(conn, &gid, &eid2)? {
            return Ok(None);
        }
        repo::find(conn, &eid2, &aid2)
    })
    .await?
    .ok_or_else(|| AppError::NotFound("附件不存在".into()))?;

    let full = crate::db::safe_join(&config.upload_dir, &att.path)?;
    let bytes = std::fs::read(&full)
        .map_err(|e| AppError::NotFound(format!("文件读取失败: {e}")))?;
    let mime = att.mime_type.clone();
    res.headers_mut().insert(
        salvo::http::header::CONTENT_TYPE,
        mime.parse().map_err(|_| AppError::Internal("bad mime".into()))?,
    );
    res.body(salvo::http::body::ResBody::Once(salvo::hyper::body::Bytes::from(bytes)));
    Ok(())
}

/// DELETE /api/v1/entities/{id}/attachments/{aid}.
///
/// # Errors
/// `NotFound`/`Unauthorized`.
#[salvo::handler]
pub async fn delete(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let entity_id = req.param::<String>("id").unwrap_or_default();
    let attachment_id = req.param::<String>("aid").unwrap_or_default();

    let (eid, aid) = (entity_id.clone(), attachment_id.clone());
    let rel = with_write(pool, move |conn| {
        if !crate::db::repo::entities::entity_exists(conn, &auth.group_id, &eid)? {
            return Err(AppError::NotFound("物品不存在".into()));
        }
        repo::delete(conn, &eid, &aid)
    })
    .await?;
    let full = crate::db::safe_join(&config.upload_dir, &rel)?;
    if full.exists() {
        let _ = std::fs::remove_file(&full);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Media guard helper (attachment token or user token).
fn require_media(config: &Config, req: &salvo::Request) -> AppResult<AuthUser> {
    crate::auth::guard::require_media(req, config)
}
