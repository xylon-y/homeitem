//! System routes: health.

use salvo::writing::Json;

/// GET /api/v1/health — liveness probe, no auth.
#[salvo::handler]
pub async fn health(res: &mut salvo::Response) {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Health {
        healthy: bool,
        version: &'static str,
    }
    res.render(Json(Health { healthy: true, version: common::APP_VERSION }));
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!common::APP_VERSION.is_empty());
    }
}
