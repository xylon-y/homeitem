//! Notifier CRUD + test-send routes.

use salvo::writing::Json;

use common::dto::{NotifierCreate, NotifierOut, NotifierTest, NotifierUpdate};

use crate::auth::guard::{json_body, require_user};
use crate::db::repo::notifiers as repo;
use crate::db::{with_read, with_write};
use crate::error::{AppError, AppResult};
use crate::router;

/// GET /api/v1/notifiers.
#[salvo::handler]
pub async fn list(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<Vec<NotifierOut>>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let out = with_read(pool, move |conn| repo::list_notifiers(conn, &gid)).await?;
    Ok(Json(out))
}

/// POST /api/v1/notifiers.
#[salvo::handler]
pub async fn create(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<NotifierOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let body: NotifierCreate = json_body(req).await?;
    let gid = auth.group_id.clone();
    let out = with_write(pool, move |conn| {
        let id = repo::create_notifier(conn, &gid, &body)?;
        repo::find_notifier(conn, &gid, &id)?
            .ok_or_else(|| AppError::Internal("notifier vanished".into()))
    })
    .await?;
    Ok(Json(out))
}

/// PUT /api/v1/notifiers/{id}.
#[salvo::handler]
pub async fn update(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<NotifierOut>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let body: NotifierUpdate = json_body(req).await?;
    let (gid, id2) = (auth.group_id.clone(), id.clone());
    let out = with_write(pool, move |conn| {
        repo::update_notifier(conn, &gid, &id2, &body)?;
        repo::find_notifier(conn, &gid, &id)?
            .ok_or_else(|| AppError::NotFound("通知器不存在".into()))
    })
    .await?;
    Ok(Json(out))
}

/// DELETE /api/v1/notifiers/{id}.
#[salvo::handler]
pub async fn delete(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let (gid, id2) = (auth.group_id.clone(), id.clone());
    with_write(pool, move |conn| repo::delete_notifier(conn, &gid, &id2)).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /api/v1/notifiers/{id}/test — send a test mail to the configured target.
#[salvo::handler]
pub async fn test(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let id = req.param::<String>("id").unwrap_or_default();
    let body: NotifierTest = json_body(req).await.unwrap_or_default();
    let gid = auth.group_id.clone();
    let notifier = with_read(pool, move |conn| repo::find_notifier(conn, &gid, &id)).await?
        .ok_or_else(|| AppError::NotFound("通知器不存在".into()))?;
    if notifier.kind != common::dto::NotifierKind::Email {
        return Err(AppError::BadRequest("仅邮件通知器支持测试发送".into()));
    }
    let mailer = crate::mail::Mailer::from_config(&config)?
        .ok_or_else(|| AppError::BadRequest("未配置 SMTP(HOMEITEM_SMTP_URL)".into()))?;
    let message = if body.message.trim().is_empty() {
        "这是一封来自 homeitem 的测试邮件。".to_string()
    } else {
        body.message
    };
    mailer.send(
        &notifier.target,
        "homeitem 测试通知",
        &message,
    ).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
