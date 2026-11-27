//! Router assembly: API routes, CORS, static hosting and catchers.

use salvo::cors::Cors;
use salvo::http::Method;
use salvo::prelude::*;

use common::API_PREFIX;

use crate::api::{attachments, entities, groups, maintenance, notifiers, system, taxonomy, users};
use crate::auth::middleware::AuthMiddleware;
use crate::config::Config;
use crate::db::Pool;

/// Depot keys for shared state.
pub mod state {
    /// Pooled database handle.
    pub const POOL: &str = "homeitem.pool";
    /// Resolved configuration.
    pub const CONFIG: &str = "homeitem.config";
}

/// Inject `Pool` + `Config` into the depot for downstream handlers.
#[derive(Clone)]
pub struct StateHoop {
    pool: Pool,
    config: Config,
}

impl StateHoop {
    /// Create the injector.
    pub fn new(pool: Pool, config: Config) -> Self {
        Self { pool, config }
    }
}

#[async_trait]
impl Handler for StateHoop {
    async fn handle(
        &self,
        _req: &mut salvo::Request,
        depot: &mut Depot,
        _res: &mut salvo::Response,
        _ctrl: &mut FlowCtrl,
    ) {
        depot.insert_typed(self.pool.clone());
        depot.insert_typed(self.config.clone());
    }
}

/// Fetch the shared pool (set by `StateHoop`).
///
/// # Panics
/// Only when a handler runs outside the state hoop — a programming error.
pub fn pool(depot: &Depot) -> &Pool {
    depot.get_typed::<Pool>().expect("state hoop must run first")
}

/// Fetch the shared config (set by `StateHoop`).
///
/// # Panics
/// Only when a handler runs outside the state hoop — a programming error.
pub fn config(depot: &Depot) -> &Config {
    depot.get_typed::<Config>().expect("state hoop must run first")
}

/// Build CORS middleware from config.
///
/// Dev mode (`HOMEITEM_DEV_CORS=1`) opens everything; otherwise only the
/// configured origins are allowed (empty list = same-origin only).
pub fn build_cors(config: &Config) -> Cors {
    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
        Method::OPTIONS,
    ];
    let mut cors = Cors::new().allow_methods(methods).allow_headers(vec!["authorization", "content-type"]);
    if config.dev_cors {
        cors = cors.allow_origin(salvo::cors::AllowOrigin::any());
        return cors;
    }
    if config.cors_origins.is_empty() {
        return cors;
    }
    // Skip malformed origins instead of panicking on startup — a bad env var
    // must never take the whole service down.
    let valid: Vec<salvo::http::HeaderValue> = config
        .cors_origins
        .iter()
        .filter_map(|o| match o.parse::<salvo::http::HeaderValue>() {
            Ok(v) => Some(v),
            Err(_) => {
                tracing::warn!(origin = %o, "ignoring malformed CORS origin");
                None
            }
        })
        .collect();
    cors.allow_origin(salvo::cors::AllowOrigin::list(valid))
}

/// Assemble the full application router.
pub fn router(pool: Pool, config: &Config) -> Router {
    let auth_guard = AuthMiddleware::new(config.clone());
    let state = StateHoop::new(pool.clone(), config.clone());

    // -- public API (no token) ------------------------------------------
    let public_api = Router::with_path(API_PREFIX)
        .hoop(state.clone())
        .push(Router::with_path("health").get(system::health))
        .push(Router::with_path("status").get(system::status))
        .push(Router::with_path("setup").post(users::setup))
        .push(Router::with_path("login").post(users::login))
        .push(Router::with_path("register").post(users::register))
        // Media downloads authenticate themselves via `require_media` (which
        // accepts 7-day attachment tokens passed in `?token=`), so they must
        // sit outside the User-scope-only auth middleware. `<img src>` cannot
        // send an Authorization header.
        .push(Router::with_path("entities/{id}/attachments/{aid}").get(attachments::download));

    // -- authenticated API ----------------------------------------------
    let api = Router::with_path(API_PREFIX)
        .hoop(state.clone())
        .hoop(auth_guard)
        .push(Router::with_path("user").get(users::me).put(users::update_me))
        .push(Router::with_path("user/password").put(users::change_password))
        .push(Router::with_path("group").get(groups::get_group).put(groups::update_group))
        .push(Router::with_path("group/members").get(groups::members))
        .push(Router::with_path("group/members/{id}").delete(groups::remove_member))
        .push(Router::with_path("group/members/{id}/role").patch(groups::set_member_role))
        .push(Router::with_path("group/invitations").get(groups::invitations).post(groups::create_invitation))
        .push(Router::with_path("group/invitations/{id}").delete(groups::delete_invitation))
        .push(Router::with_path("entities").get(entities::list_entities).post(entities::create_entity))
        .push(
            Router::with_path("entities/{id}")
                .get(entities::get_entity)
                .put(entities::update_entity)
                .patch(entities::patch_entity)
                .delete(entities::delete_entity),
        )
        .push(Router::with_path("entities/{id}/duplicate").post(entities::duplicate_entity))
        .push(Router::with_path("entities/{id}/path").get(entities::entity_path))
        .push(
            Router::with_path("entities/{id}/attachments")
                .get(attachments::list)
                .post(attachments::upload),
        )
        .push(Router::with_path("entities/{id}/attachments/{aid}").delete(attachments::delete))
        .push(
            Router::with_path("entities/{id}/maintenance")
                .get(maintenance::list)
                .post(maintenance::create),
        )
        .push(
            Router::with_path("entities/{id}/maintenance/{mid}")
                .put(maintenance::update)
                .delete(maintenance::delete),
        )
        .push(Router::with_path("locations/tree").get(entities::location_tree))
        .push(
            Router::with_path("entity-types")
                .get(entities::entity_types)
                .post(taxonomy::create_type),
        )
        .push(
            Router::with_path("entity-types/{id}")
                .put(taxonomy::update_type)
                .delete(taxonomy::delete_type),
        )
        .push(
            Router::with_path("tags")
                .get(entities::list_tags)
                .post(taxonomy::create_tag),
        )
        .push(Router::with_path("tags/tree").get(taxonomy::tag_tree))
        .push(
            Router::with_path("tags/{id}")
                .put(taxonomy::update_tag)
                .delete(taxonomy::delete_tag),
        )
        .push(
            Router::with_path("notifiers")
                .get(notifiers::list)
                .post(notifiers::create),
        )
        .push(
            Router::with_path("notifiers/{id}")
                .put(notifiers::update)
                .delete(notifiers::delete),
        )
        .push(Router::with_path("notifiers/{id}/test").post(notifiers::test))
        .push(Router::with_path("reminders").get(entities::list_reminders))
        .push(Router::with_path("statistics").get(entities::get_statistics))
        .push(Router::with_path("statistics/by-location").get(entities::stats_by_location))
        .push(Router::with_path("statistics/by-tag").get(entities::stats_by_tag))
        .push(Router::with_path("statistics/value-over-time").get(entities::stats_value_over_time));

    // static SPA hosting + history fallback (needs state hoop for static_dir)
    let static_router = crate::assets::static_router();

    Router::new()
        .hoop(StateHoop::new(pool, config.clone()))
        .push(public_api)
        .push(api)
        .push(static_router)
        .push(Router::with_path("{**rest}").get(crate::assets::spa_fallback))
}
