//! System routes: health + setup status.

use salvo::writing::Json;

use crate::db::{with_read, Pool};
use crate::router;

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

/// GET /api/v1/status — whether first-run setup is still needed (no auth).
#[salvo::handler]
pub async fn status(depot: &mut salvo::Depot) -> crate::error::AppResult<Json<serde_json::Value>> {
    let pool: &Pool = router::pool(depot);
    let needs_setup = with_read(pool, |conn| crate::db::repo::users::any_user_exists(conn)).await?;
    Ok(Json(serde_json::json!({
        "needsSetup": !needs_setup,
        "version": common::APP_VERSION,
        "registrationEnabled": crate::router::config(depot).allow_registration,
    })))
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!common::APP_VERSION.is_empty());
    }
}
