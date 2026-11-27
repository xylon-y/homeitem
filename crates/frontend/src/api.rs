//! REST 客户端：所有后端交互集中在这里。

use common::dto::{
    EntityCreate, EntityOut, EntityPatch, EntitySummary, EntityTypeCreate, EntityTypeOut,
    EntityTypeUpdate, GroupOut, InvitationOut, MaintenanceCreate,
    MaintenanceEntry, MaintenanceUpdate, MemberOut, NotifierCreate, NotifierOut, NotifierUpdate,
    Page, ReminderResponse, Statistics, TagCreate, TagOut, TagSummary, TagUpdate, TokenResponse,
    TotalsByOrganizer, TreeItem, UserOut, ValueOverTimeEntry,
};
use common::GroupRole;
use gloo_net::http::Request;

#[derive(Debug, Clone)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// 登录态信号（401 时置 false，驱动界面重算）。
static UNAUTHORIZED_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn take_unauthorized() -> bool {
    UNAUTHORIZED_FLAG.swap(false, std::sync::atomic::Ordering::SeqCst)
}

/// 当前令牌存取（localStorage）。
pub mod token {
    const KEY: &str = "homeitem_token";
    const MEDIA_KEY: &str = "homeitem_media_token";

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn get() -> Option<String> {
        storage()?.get_item(KEY).ok()?
    }

    pub fn set(main: &str, media: &str) {
        if let Some(store) = storage() {
            let _ = store.set_item(KEY, main);
            let _ = store.set_item(MEDIA_KEY, media);
        }
    }

    pub fn media() -> Option<String> {
        storage()?.get_item(MEDIA_KEY).ok()?
    }

    pub fn clear() {
        if let Some(store) = storage() {
            let _ = store.remove_item(KEY);
            let _ = store.remove_item(MEDIA_KEY);
        }
    }
}

fn url(path: &str) -> String {
    format!("{}{}", common::API_PREFIX, path)
}

async fn parse<T: serde::de::DeserializeOwned>(resp: gloo_net::http::Response, is_login: bool) -> Result<T, ApiError> {
    let status = resp.status();
    let body = resp.text().await.map_err(|e| ApiError {
        code: "NETWORK".into(),
        message: format!("网络错误: {e}"),
    })?;
    if status == 401 && !is_login {
        token::clear();
        UNAUTHORIZED_FLAG.store(true, std::sync::atomic::Ordering::SeqCst);
        return Err(ApiError { code: "UNAUTHORIZED".into(), message: "登录已过期，请重新登录".into() });
    }
    let value: Result<serde_json::Value, _> = serde_json::from_str(&body);
    let err_payload = |v: serde_json::Value| ApiError {
        code: v.get("code").and_then(|c| c.as_str()).unwrap_or("ERROR").into(),
        message: v.get("error").and_then(|e| e.as_str()).unwrap_or("请求失败").into(),
    };
    if !(200..300).contains(&status) {
        return Err(match value {
            Ok(v) => err_payload(v),
            Err(_) => ApiError { code: "ERROR".into(), message: format!("请求失败 (HTTP {status})") },
        });
    }
    value
        .map_err(|_| ApiError { code: "ERROR".into(), message: format!("响应解析失败: {body}") })
        .and_then(|v| serde_json::from_value(v).map_err(|e| ApiError { code: "ERROR".into(), message: format!("数据格式错误: {e}") }))
}

async fn request<T: serde::de::DeserializeOwned>(
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
    is_login: bool,
) -> Result<T, ApiError> {
    let mut req = match method {
        "POST" => Request::post(&url(path)),
        "PUT" => Request::put(&url(path)),
        "PATCH" => Request::patch(&url(path)),
        "DELETE" => Request::delete(&url(path)),
        _ => Request::get(&url(path)),
    };
    if let Some(t) = token::get() {
        req = req.header("Authorization", &format!("Bearer {t}"));
    }
    let resp = if let Some(b) = body {
        let payload = b.to_string();
        req.header("Content-Type", "application/json")
            .body(&payload)
            .map_err(|e| ApiError { code: "BAD".into(), message: format!("请求构造失败: {e}") })?
            .send()
            .await
    } else {
        req.send().await
    }
    .map_err(|e| ApiError {
        code: "NETWORK".into(),
        message: format!("网络错误: {e}"),
    })?;
    parse(resp, is_login).await
}

pub async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    request("GET", path, None, false).await
}

pub async fn send_json<T: serde::de::DeserializeOwned>(
    method: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<T, ApiError> {
    request(method, path, Some(body), false).await
}

// ---- 认证 / 用户 ----

pub async fn status() -> Result<serde_json::Value, ApiError> {
    get_json("/status").await
}

pub async fn login(username: &str, password: &str, stay: bool) -> Result<TokenResponse, ApiError> {
    let v = serde_json::json!({ "username": username, "password": password, "stayLoggedIn": stay });
    let t: TokenResponse = request("POST", "/login", Some(&v), true).await?;
    token::set(&t.token, &t.attachment_token);
    Ok(t)
}

pub async fn setup(name: &str, email: &str, password: &str) -> Result<TokenResponse, ApiError> {
    let v = serde_json::json!({ "name": name, "email": email, "password": password });
    let t: TokenResponse = request("POST", "/setup", Some(&v), true).await?;
    token::set(&t.token, &t.attachment_token);
    Ok(t)
}

pub async fn register(name: &str, email: &str, password: &str, invite: &str) -> Result<TokenResponse, ApiError> {
    let v = serde_json::json!({ "name": name, "email": email, "password": password, "token": invite });
    let t: TokenResponse = request("POST", "/register", Some(&v), true).await?;
    token::set(&t.token, &t.attachment_token);
    Ok(t)
}

pub async fn me() -> Result<UserOut, ApiError> {
    get_json("/user").await
}

pub async fn update_me(name: Option<&str>, email: Option<&str>) -> Result<UserOut, ApiError> {
    send_json("PUT", "/user", &serde_json::json!({ "name": name, "email": email })).await
}

pub async fn change_password(current: &str, new: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("PUT", "/user/password", &serde_json::json!({ "current": current, "new": new })).await?;
    Ok(())
}

pub fn logout() {
    token::clear();
}

// ---- 组 / 成员 / 邀请 ----

pub async fn group() -> Result<GroupOut, ApiError> { get_json("/group").await }

pub async fn update_group(name: &str, currency: &str) -> Result<GroupOut, ApiError> {
    send_json("PUT", "/group", &serde_json::json!({ "name": name, "currency": currency })).await
}

pub async fn members() -> Result<Vec<MemberOut>, ApiError> { get_json("/group/members").await }

pub async fn remove_member(id: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("DELETE", &format!("/group/members/{id}"), &serde_json::json!({})).await?;
    Ok(())
}

pub async fn set_member_role(id: &str, role: GroupRole) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("PATCH", &format!("/group/members/{id}/role"), &serde_json::json!({ "role": role.to_string() })).await?;
    Ok(())
}

pub async fn invitations() -> Result<Vec<InvitationOut>, ApiError> { get_json("/group/invitations").await }

pub async fn create_invitation(role: GroupRole, email: Option<&str>) -> Result<InvitationOut, ApiError> {
    send_json("POST", "/group/invitations", &serde_json::json!({ "role": role.to_string(), "email": email })).await
}

pub async fn delete_invitation(id: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("DELETE", &format!("/group/invitations/{id}"), &serde_json::json!({})).await?;
    Ok(())
}

// ---- 物品 ----

pub async fn entities(q: &str) -> Result<Page<EntitySummary>, ApiError> {
    let suffix = if q.is_empty() { String::new() } else { format!("?{q}") };
    get_json(&format!("/entities{suffix}")).await
}

pub async fn entity(id: &str) -> Result<EntityOut, ApiError> { get_json(&format!("/entities/{id}")).await }

pub async fn create_entity(req: &EntityCreate) -> Result<EntityOut, ApiError> {
    send_json("POST", "/entities", &serde_json::to_value(req).unwrap()).await
}

pub async fn update_entity(id: &str, req: &serde_json::Value) -> Result<EntityOut, ApiError> {
    send_json("PUT", &format!("/entities/{id}"), req).await
}

pub async fn patch_entity(id: &str, req: &EntityPatch) -> Result<EntityOut, ApiError> {
    send_json("PATCH", &format!("/entities/{id}"), &serde_json::to_value(req).unwrap()).await
}

pub async fn delete_entity(id: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("DELETE", &format!("/entities/{id}"), &serde_json::json!({})).await?;
    Ok(())
}

pub async fn duplicate_entity(id: &str) -> Result<EntityOut, ApiError> {
    send_json("POST", &format!("/entities/{id}/duplicate"), &serde_json::json!({
        "copyMaintenance": true, "copyAttachments": true, "copyCustomFields": true, "copyPrefix": "副本 "
    })).await
}

pub async fn entity_path(id: &str) -> Result<Vec<common::dto::EntityPath>, ApiError> {
    get_json(&format!("/entities/{id}/path")).await
}

pub async fn locations_tree() -> Result<Vec<TreeItem>, ApiError> { get_json("/locations/tree").await }
pub async fn entity_types() -> Result<Vec<EntityTypeOut>, ApiError> { get_json("/entity-types").await }
pub async fn tags() -> Result<Vec<TagSummary>, ApiError> { get_json("/tags").await }
pub async fn tags_tree() -> Result<Vec<TagOut>, ApiError> { get_json("/tags/tree").await }

// ---- 分类管理 ----

pub async fn create_tag(req: &TagCreate) -> Result<TagSummary, ApiError> {
    send_json("POST", "/tags", &serde_json::to_value(req).unwrap()).await
}
pub async fn update_tag(id: &str, req: &TagUpdate) -> Result<TagSummary, ApiError> {
    send_json("PUT", &format!("/tags/{id}"), &serde_json::to_value(req).unwrap()).await
}
pub async fn delete_tag(id: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("DELETE", &format!("/tags/{id}"), &serde_json::json!({})).await?;
    Ok(())
}
pub async fn create_type(req: &EntityTypeCreate) -> Result<EntityTypeOut, ApiError> {
    send_json("POST", "/entity-types", &serde_json::to_value(req).unwrap()).await
}
pub async fn update_type(id: &str, req: &EntityTypeUpdate) -> Result<EntityTypeOut, ApiError> {
    send_json("PUT", &format!("/entity-types/{id}"), &serde_json::to_value(req).unwrap()).await
}
pub async fn delete_type(id: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("DELETE", &format!("/entity-types/{id}"), &serde_json::json!({})).await?;
    Ok(())
}

// ---- 附件 ----

pub async fn attachments(entity_id: &str) -> Result<Vec<common::dto::ItemAttachment>, ApiError> {
    get_json(&format!("/entities/{entity_id}/attachments")).await
}

pub async fn delete_attachment(entity_id: &str, aid: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("DELETE", &format!("/entities/{entity_id}/attachments/{aid}"), &serde_json::json!({})).await?;
    Ok(())
}

/// 附件下载 URL（浏览器 <img src> 用媒体令牌）。
pub fn attachment_url(entity_id: &str, aid: &str) -> String {
    let token = token::media().unwrap_or_default();
    format!("/api/v1/entities/{entity_id}/attachments/{aid}?token={token}")
}

pub async fn upload_attachment(entity_id: &str, file_bytes: Vec<u8>, filename: &str, mime: &str, kind: &str, title: &str) -> Result<common::dto::ItemAttachment, ApiError> {
    let t = token::get().unwrap_or_default();
    let boundary = format!("----homeitem{}", js_sys::Date::now() as i64);
    let mut body = Vec::new();
    let push = |s: &str, b: &mut Vec<u8>| b.extend_from_slice(s.as_bytes());
    push(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"type\"\r\n\r\n{kind}\r\n"), &mut body);
    push(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\n{title}\r\n"), &mut body);
    push(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n"), &mut body);
    body.extend_from_slice(&file_bytes);
    push(&format!("\r\n--{boundary}--\r\n"), &mut body);
    let resp = Request::post(&url(&format!("/entities/{entity_id}/attachments")))
        .header("Authorization", &format!("Bearer {t}"))
        .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .body(body)
        .map_err(|e| ApiError { code: "BAD".into(), message: format!("{e}") })?
        .send().await
        .map_err(|e| ApiError { code: "NETWORK".into(), message: format!("{e}") })?;
    parse(resp, false).await
}

// ---- 保养记录 ----

pub async fn maintenance(entity_id: &str, status: &str) -> Result<Vec<MaintenanceEntry>, ApiError> {
    get_json(&format!("/entities/{entity_id}/maintenance?status={status}")).await
}
pub async fn create_maintenance(entity_id: &str, req: &MaintenanceCreate) -> Result<MaintenanceEntry, ApiError> {
    send_json("POST", &format!("/entities/{entity_id}/maintenance"), &serde_json::to_value(req).unwrap()).await
}
pub async fn update_maintenance(entity_id: &str, id: &str, req: &MaintenanceUpdate) -> Result<MaintenanceEntry, ApiError> {
    send_json("PUT", &format!("/entities/{entity_id}/maintenance/{id}"), &serde_json::to_value(req).unwrap()).await
}
pub async fn delete_maintenance(entity_id: &str, id: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("DELETE", &format!("/entities/{entity_id}/maintenance/{id}"), &serde_json::json!({})).await?;
    Ok(())
}

// ---- 提醒 / 统计 / 通知器 ----

pub async fn reminders(q: &str) -> Result<ReminderResponse, ApiError> {
    let suffix = if q.is_empty() { String::new() } else { format!("?{q}") };
    get_json(&format!("/reminders{suffix}")).await
}
pub async fn statistics() -> Result<Statistics, ApiError> { get_json("/statistics").await }
pub async fn stats_by_location() -> Result<Vec<TotalsByOrganizer>, ApiError> { get_json("/statistics/by-location").await }
pub async fn stats_by_tag() -> Result<Vec<TotalsByOrganizer>, ApiError> { get_json("/statistics/by-tag").await }
pub async fn stats_value_over_time() -> Result<Vec<ValueOverTimeEntry>, ApiError> { get_json("/statistics/value-over-time").await }

pub async fn notifiers() -> Result<Vec<NotifierOut>, ApiError> { get_json("/notifiers").await }
pub async fn create_notifier(req: &NotifierCreate) -> Result<NotifierOut, ApiError> {
    send_json("POST", "/notifiers", &serde_json::to_value(req).unwrap()).await
}
pub async fn update_notifier(id: &str, req: &NotifierUpdate) -> Result<NotifierOut, ApiError> {
    send_json("PUT", &format!("/notifiers/{id}"), &serde_json::to_value(req).unwrap()).await
}
pub async fn delete_notifier(id: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("DELETE", &format!("/notifiers/{id}"), &serde_json::json!({})).await?;
    Ok(())
}
pub async fn test_notifier(id: &str, message: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = send_json("POST", &format!("/notifiers/{id}/test"), &serde_json::json!({ "message": message })).await?;
    Ok(())
}

/// 轻量 URL 编码（避免额外依赖）。
pub fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ---- CSV 导入导出 ----

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ImportResult {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ImportPreview {
    pub total: usize,
    pub warnings: Vec<String>,
    pub preview: Vec<serde_json::Value>,
}

fn build_multipart(filename: &str, file_bytes: Vec<u8>) -> (String, Vec<u8>) {
    let boundary = format!("----homeitem{}", js_sys::Date::now() as i64);
    let mut body = Vec::new();
    let push = |s: &str, b: &mut Vec<u8>| b.extend_from_slice(s.as_bytes());
    push(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: text/csv\r\n\r\n"), &mut body);
    body.extend_from_slice(&file_bytes);
    push(&format!("\r\n--{boundary}--\r\n"), &mut body);
    (boundary, body)
}

async fn post_file<T: serde::de::DeserializeOwned>(path: &str, filename: &str, bytes: Vec<u8>) -> Result<T, ApiError> {
    let (boundary, body) = build_multipart(filename, bytes);
    let t = token::get().unwrap_or_default();
    let resp = Request::post(&url(path))
        .header("Authorization", &format!("Bearer {t}"))
        .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .body(body)
        .map_err(|e| ApiError { code: "BAD".into(), message: format!("{e}") })?
        .send().await
        .map_err(|e| ApiError { code: "NETWORK".into(), message: format!("{e}") })?;
    parse(resp, false).await
}

pub async fn preview_import(filename: &str, bytes: Vec<u8>) -> Result<ImportPreview, ApiError> {
    post_file("/entities/import/preview", filename, bytes).await
}

pub async fn import_csv(filename: &str, bytes: Vec<u8>) -> Result<ImportResult, ApiError> {
    post_file("/entities/import", filename, bytes).await
}

/// 触发浏览器下载 CSV（带认证头，通过 fetch + blob）。
pub async fn export_csv() -> Result<(), ApiError> {
    use wasm_bindgen::JsCast;
    let t = token::get().unwrap_or_default();
    let resp = gloo_net::http::Request::get(&url("/entities/export"))
        .header("Authorization", &format!("Bearer {t}"))
        .send().await
        .map_err(|e| ApiError { code: "NETWORK".into(), message: format!("{e}") })?;
    if resp.status() != 200 {
        return Err(ApiError { code: "ERROR".into(), message: format!("导出失败 (HTTP {})", resp.status()) });
    }
    let bytes = resp.binary().await.map_err(|e| ApiError { code: "ERROR".into(), message: format!("{e}") })?;
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    let blob = web_sys::Blob::new_with_u8_array_sequence(&js_sys::Array::of1(&array))
        .map_err(|e| ApiError { code: "ERROR".into(), message: format!("{:?}", e) })?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| ApiError { code: "ERROR".into(), message: format!("{:?}", e) })?;
    let window = web_sys::window().ok_or_else(|| ApiError { code: "ERROR".into(), message: "无窗口".into() })?;
    let doc = window.document().ok_or_else(|| ApiError { code: "ERROR".into(), message: "无文档".into() })?;
    let a = doc.create_element("a").map_err(|e| ApiError { code: "ERROR".into(), message: format!("{:?}", e) })?
        .dyn_into::<web_sys::HtmlAnchorElement>().map_err(|_| ApiError { code: "ERROR".into(), message: "元素转换失败".into() })?;
    a.set_href(&url);
    a.set_download("homeitem-export.csv");
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}
