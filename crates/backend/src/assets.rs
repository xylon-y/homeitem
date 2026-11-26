//! Static hosting: embedded frontend (rust-embed) with SPA fallback, plus
//! optional on-disk override via HOMEITEM_STATIC_DIR for development.

use rust_embed::RustEmbed;
use salvo::http::StatusCode;
use salvo::prelude::*;

/// Embedded frontend build output (crates/frontend/dist).
#[derive(RustEmbed)]
#[folder = "../../crates/frontend/dist/"]
struct FrontendAssets;

/// Serve embedded SPA under any path; the state hoop must run before it.
#[salvo::handler]
pub async fn serve_asset(depot: &mut Depot, req: &mut Request, res: &mut Response) {
    let config = crate::router::config(depot);
    let raw = req
        .uri()
        .path()
        .trim_start_matches('/')
        .trim_start_matches("api/v1/");
    let clean = sanitize(raw);

    // dev override: serve from disk first
    if let Some(dir) = &config.static_dir {
        let full = dir.join(&clean);
        if full.is_file() {
            if let Ok(bytes) = std::fs::read(&full) {
                let mime = mime_guess::from_path(&full).first_or_octet_stream();
                res.status_code(StatusCode::OK);
                res.headers_mut().insert(
                    salvo::http::header::CONTENT_TYPE,
                    mime.as_ref().parse().expect("valid mime"),
                );
                res.body(salvo::http::body::ResBody::Once(salvo::hyper::body::Bytes::from(bytes)));
                return;
            }
        }
    }

    let path = if clean.is_empty() {
        "index.html".to_string()
    } else {
        clean
    };
    if let Some(file) = FrontendAssets::get(&path) {
        let mime = mime_guess::from_path(&path).first_or_octet_stream();
        res.status_code(StatusCode::OK);
        res.headers_mut().insert(
            salvo::http::header::CONTENT_TYPE,
            mime.as_ref().parse().expect("valid mime"),
        );
        res.body(salvo::http::body::ResBody::Once(salvo::hyper::body::Bytes::from(
            file.data.into_owned(),
        )));
        return;
    }
    // SPA fallback for client-side routes
    serve_index(res);
}

/// History-mode fallback: always answer index.html.
#[salvo::handler]
pub async fn spa_fallback(_req: &mut Request, res: &mut Response) {
    serve_index(res);
}

fn serve_index(res: &mut Response) {
    if let Some(index) = FrontendAssets::get("index.html") {
        res.status_code(StatusCode::OK);
        res.headers_mut().insert(
            salvo::http::header::CONTENT_TYPE,
            "text/html; charset=utf-8".parse().expect("valid mime"),
        );
        res.body(salvo::http::body::ResBody::Once(salvo::hyper::body::Bytes::from(
            index.data.into_owned(),
        )));
    } else {
        res.status_code(StatusCode::NOT_FOUND);
        res.body(salvo::http::body::ResBody::Once(salvo::hyper::body::Bytes::from(
            "frontend build missing (crates/frontend/dist)",
        )));
    }
}

/// Strip traversal segments; returns a relative asset path.
fn sanitize(path: &str) -> String {
    path.split('/')
        .filter(|seg| !seg.is_empty() && *seg != "." && *seg != "..")
        .collect::<Vec<_>>()
        .join("/")
}

/// Static file router mounted after the API routes.
pub fn static_router() -> Router {
    Router::new().push(Router::with_path("{**rest}").get(serve_asset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_blocks_traversal() {
        assert_eq!(sanitize("../../etc/passwd"), "etc/passwd");
        assert_eq!(sanitize("a//b/./c"), "a/b/c");
        assert_eq!(sanitize(""), "");
    }

    #[test]
    fn index_html_is_embedded() {
        // build.rs guarantees the placeholder exists
        assert!(FrontendAssets::get("index.html").is_some());
    }
}
