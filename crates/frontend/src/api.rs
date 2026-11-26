//! REST 客户端：所有后端交互集中在这里。

use common::dto::{EntitySummary, Page, Statistics, TokenResponse, TreeItem, UserOut};
use common::API_PREFIX;
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

/// 当前令牌存取（localStorage）。
pub mod token {
    const KEY: &str = "homeitem_token";
    const MEDIA_KEY: &str = "homeitem_media_token";

    fn storage() -> Option<web_sys::Storage> {
        let window = web_sys::window()?;
        window.local_storage().ok()?
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

fn base() -> String {
    String::new() // 同源部署
}

fn url(path: &str) -> String {
    format!("{}{}{}", base(), common::API_PREFIX, path)
}

async fn parse<T: serde::de::DeserializeOwned>(resp: gloo_net::http::Response) -> Result<T, ApiError> {
    let status = resp.status();
    let body = resp.text().await.map_err(|e| ApiError {
        code: "NETWORK".into(),
        message: format!("网络错误: {e}"),
    })?;
    if status == 401 {
        token::clear();
        return Err(ApiError { code: "UNAUTHORIZED".into(), message: "登录已过期，请重新登录".into() });
    }
    let value: Result<serde_json::Value, _> = serde_json::from_str(&body);
    let err_payload = |v: serde_json::Value| ApiError {
        code: v.get("code").and_then(|c| c.as_str()).unwrap_or("ERROR").into(),
        message: v.get("error").and_then(|e| e.as_str()).unwrap_or("请求失败").into(),
    };
    if !(200..300).contains(&status) {
        let err = match value {
            Ok(v) => err_payload(v),
            Err(_) => ApiError { code: "ERROR".into(), message: "响应解析失败".into() },
        };
        return Err(err);
    }
    value
        .map_err(|_| ApiError { code: "ERROR".into(), message: "响应解析失败".into() })
        .and_then(|v| serde_json::from_value(v).map_err(|e| ApiError { code: "ERROR".into(), message: format!("数据格式错误: {e}") }))
}

/// GET JSON with auth header.
pub async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let mut req = Request::get(&url(path));
    if let Some(t) = token::get() {
        req = req.header("Authorization", &format!("Bearer {t}"));
    }
    let resp = req.send().await.map_err(|e| ApiError {
        code: "NETWORK".into(),
        message: format!("网络错误: {e}"),
    })?;
    parse(resp).await
}

/// POST/PUT/PATCH/DELETE JSON with auth header.
pub async fn send_json<T: serde::de::DeserializeOwned>(
    method: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<T, ApiError> {
    let url = url(path);
    let mut req = match method {
        "POST" => Request::post(&url),
        "PUT" => Request::put(&url),
        "PATCH" => Request::patch(&url),
        "DELETE" => Request::delete(&url),
        _ => Request::get(&url),
    };
    if let Some(t) = token::get() {
        req = req.header("Authorization", &format!("Bearer {t}"));
    }
    let resp = req
        .header("Content-Type", "application/json")
        .body(&body.to_string())
        .map_err(|e| ApiError { code: "BAD".into(), message: format!("请求构造失败: {e}") })?
        .send()
        .await
        .map_err(|e| ApiError { code: "NETWORK".into(), message: format!("网络错误: {e}") })?;
    parse(resp).await
}

// ---- 域操作 ----

pub async fn login(username: &str, password: &str, stay: bool) -> Result<TokenResponse, ApiError> {
    let v = serde_json::json!({ "username": username, "password": password, "stayLoggedIn": stay });
    let t: TokenResponse = send_json("POST", "/login", &v).await?;
    token::set(&t.token, &t.attachment_token);
    Ok(t)
}

pub async fn setup(name: &str, email: &str, password: &str) -> Result<TokenResponse, ApiError> {
    let v = serde_json::json!({ "name": name, "email": email, "password": password });
    let t: TokenResponse = send_json("POST", "/setup", &v).await?;
    token::set(&t.token, &t.attachment_token);
    Ok(t)
}

pub async fn me() -> Result<UserOut, ApiError> {
    get_json("/user").await
}

pub async fn entities(q: &str) -> Result<Page<EntitySummary>, ApiError> {
    let suffix = if q.is_empty() { String::new() } else { format!("?{q}") };
    get_json(&format!("/entities{suffix}")).await
}

pub async fn locations_tree() -> Result<Vec<TreeItem>, ApiError> {
    get_json("/locations/tree").await
}

pub async fn statistics() -> Result<Statistics, ApiError> {
    get_json("/statistics").await
}

pub async fn logout() {
    token::clear();
}
