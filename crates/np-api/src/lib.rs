//! The NextPlane HTTP API. Every route lives under `/api/v1` (SPEC §8).

pub mod auth;
pub mod automations;
pub mod error;
pub mod events;
pub mod extract;
pub mod mail;
pub mod mcp;
pub mod oidc;
pub mod origin;
pub mod pagehtml;
pub mod pagesync;
pub mod providers;
pub mod ratelimit;
pub mod richtext;
pub mod services;
mod routes;
mod system;
pub mod util;
pub mod webhooks;
pub mod xlsx;

pub use routes::inbox::send_digests;
pub use routes::setup::issue_setup_token;

use axum::Router;
use std::sync::Arc;

use axum::middleware;
use axum::routing::{any, get, post};
use np_store::Store;
use serde::Serialize;
use tower_http::trace::TraceLayer;

pub use error::ApiError;

/// Build information reported by `/api/v1/version`.
#[derive(Debug, Clone, Serialize)]
pub struct VersionInfo {
    pub version: String,
    pub commit: String,
    pub build_date: String,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub version: VersionInfo,
    pub limiter: Arc<ratelimit::RateLimiter>,
    pub mailer: Option<mail::Mailer>,
    /// The address people use to reach this server (`https://plane.example.com`), for links in
    /// emails and copied invite links. Without it, links use the request's own host.
    pub public_url: Option<String>,
    /// Where uploaded files are written (`<data>/uploads`).
    pub uploads_dir: std::path::PathBuf,
    /// Largest accepted upload, in bytes.
    pub max_upload: usize,
    pub events: events::Bus,
    /// The finished API router, so MCP tools can call the REST API as their caller.
    pub inner: Arc<std::sync::OnceLock<Router>>,
    /// Single sign-on, when `[oidc]` is in config.toml.
    pub oidc: Option<oidc::OidcConfig>,
    pub oidc_pending: Arc<oidc::PendingLogins>,
    pub http: reqwest::Client,
    /// Pages being co-edited right now.
    pub rooms: Arc<pagesync::Rooms>,
    /// Unsplash's API (tests point it elsewhere).
    pub unsplash_api: String,
}

impl AppState {
    pub fn new(store: Store, version: VersionInfo) -> Self {
        Self {
            store,
            version,
            limiter: Arc::default(),
            mailer: None,
            public_url: None,
            uploads_dir: std::env::temp_dir().join("nextplane-uploads"),
            max_upload: 25 * 1024 * 1024,
            events: events::Bus::default(),
            inner: Arc::default(),
            oidc: None,
            oidc_pending: Arc::default(),
            http: reqwest::Client::builder()
                .user_agent("NextPlane")
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("HTTP client"),
            rooms: Arc::default(),
            unsplash_api: "https://api.unsplash.com".into(),
        }
    }

    /// The base URL for links: `public_url`, else scheme and host of this request.
    pub fn base_url(&self, headers: &axum::http::HeaderMap) -> String {
        if let Some(u) = &self.public_url {
            return u.trim_end_matches('/').to_string();
        }
        let host = headers
            .get("x-forwarded-host")
            .or_else(|| headers.get(axum::http::header::HOST))
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(str::trim)
            .unwrap_or("localhost");
        let scheme = if auth::is_https(headers) {
            "https"
        } else {
            "http"
        };
        format!("{scheme}://{host}")
    }
}

/// The full app: the API under `/api`, and `spa` (the embedded web app) for every other path.
pub fn router(state: AppState, spa: Router) -> Router {
    use axum::routing::{delete, patch};
    use routes::{
        account, admin, analytics, assist, auth, collab, cycles, home, inbox, intake, integrations, invites,
        members, modules, pages, profile, projects, setup, sso, tokens, transfer, views, work_items as wi,
        workspaces,
    };
    let api = Router::new()
        .route("/health", get(system::health))
        .route("/version", get(system::version))
        .route("/instance", get(setup::instance))
        .route("/setup/verify", post(setup::verify))
        .route("/setup", post(setup::complete))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/me", get(auth::me).patch(account::update_profile))
        .route("/me/password", post(account::change_password))
        .route("/auth/signup", post(account::signup))
        .route("/auth/forgot", post(account::forgot))
        .route("/auth/oidc/start", get(sso::start))
        .route("/auth/oidc/callback", get(sso::callback))
        .route("/auth/oauth/{provider}/start", get(sso::oauth_start))
        .route("/auth/oauth/{provider}/callback", get(sso::oauth_callback))
        .route(
            "/auth/reset/{token}",
            get(account::reset_info).post(account::reset),
        )
        .route(
            "/workspaces",
            get(workspaces::list).post(workspaces::create),
        )
        .route("/workspaces/trash", get(workspaces::trash))
        .route("/workspaces/slug-check", get(workspaces::slug_check))
        .route(
            "/workspaces/{slug}",
            get(workspaces::get)
                .patch(workspaces::update)
                .delete(workspaces::remove),
        )
        .route("/workspaces/{slug}/restore", post(workspaces::restore))
        .route("/workspaces/{slug}/members", get(members::list))
        .route(
            "/workspaces/{slug}/members/{user_id}",
            patch(members::set_role).delete(members::remove),
        )
        .route("/workspaces/{slug}/leave", post(members::leave))
        .route(
            "/workspaces/{slug}/members/{user_id}/profile",
            get(profile::profile),
        )
        .route(
            "/workspaces/{slug}/members/{user_id}/activity",
            get(profile::activity),
        )
        .route(
            "/workspaces/{slug}/invites",
            get(members::invites).post(members::invite),
        )
        .route("/workspaces/{slug}/invites/{id}", delete(members::revoke))
        .route(
            "/workspaces/{slug}/projects",
            get(projects::list).post(projects::create),
        )
        .route(
            "/workspaces/{slug}/projects/{id}",
            get(projects::get)
                .patch(projects::update)
                .delete(projects::remove),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/archive",
            post(projects::archive),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/unarchive",
            post(projects::unarchive),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/join",
            post(projects::join),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/leave",
            post(projects::leave),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/members",
            get(projects::members).post(projects::add_member),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/members/{user_id}",
            patch(projects::set_member_role).delete(projects::remove_member),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/states",
            get(projects::states).post(projects::create_state),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/states/{state_id}",
            patch(projects::update_state).delete(projects::delete_state),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/labels",
            get(projects::labels).post(projects::create_label),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/labels/{label_id}",
            patch(projects::update_label).delete(projects::delete_label),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items",
            get(wi::list).post(wi::create).patch(wi::bulk_update),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}",
            get(wi::get).patch(wi::update).delete(wi::remove),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/archive",
            post(wi::archive),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/unarchive",
            post(wi::unarchive),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/activity",
            get(wi::activity),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/relations",
            get(wi::relations).post(wi::add_relation),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/relations/{rid}",
            delete(wi::remove_relation),
        )
        .route(
            "/workspaces/{slug}/work-items/by-key/{key}",
            get(wi::by_key),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/comments",
            get(collab::comments).post(collab::add_comment),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/comments/{cid}",
            patch(collab::edit_comment).delete(collab::delete_comment),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/comments/{cid}/hide",
            post(collab::hide_comment),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/comments/{cid}/unhide",
            post(collab::unhide_comment),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/comments/{cid}/reactions",
            post(collab::react_comment),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/reactions",
            get(collab::item_reactions).post(collab::react_item),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/attachments",
            get(collab::attachments).post(collab::upload_attachment),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/attachments/{fid}",
            delete(collab::delete_attachment),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/links",
            get(collab::links).post(collab::add_link),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/links/{lid}",
            delete(collab::remove_link),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/subscription",
            get(collab::subscription)
                .post(collab::subscribe)
                .delete(collab::unsubscribe),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/uploads",
            post(collab::upload_editor_image),
        )
        .route("/workspaces/{slug}/logo", post(collab::upload_logo))
        .route("/me/avatar", post(collab::upload_avatar))
        .route("/files/{fid}", get(collab::download))
        .route("/ws", get(events::ws))
        .route("/me/tokens", get(tokens::list).post(tokens::create))
        .route("/me/tokens/{id}", delete(tokens::revoke))
        .route(
            "/workspaces/{slug}/bots",
            get(tokens::bots).post(tokens::create_bot),
        )
        .route("/workspaces/{slug}/bots/{id}", delete(tokens::remove_bot))
        .route(
            "/workspaces/{slug}/bots/{id}/token",
            post(tokens::rotate_bot_token),
        )
        .route("/workspaces/{slug}/search", get(tokens::search))
        .route(
            "/workspaces/{slug}/projects/{id}/cycles",
            get(cycles::list).post(cycles::create),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/cycles/{cid}",
            get(cycles::get)
                .patch(cycles::update)
                .delete(cycles::remove),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/cycles/{cid}/archive",
            post(cycles::archive),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/cycles/{cid}/unarchive",
            post(cycles::unarchive),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/cycles/{cid}/progress",
            get(cycles::progress),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/cycles/{cid}/transfer",
            post(cycles::transfer),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/views",
            get(views::list).post(views::create),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/views/{vid}",
            patch(views::update).delete(views::remove),
        )
        .route("/workspaces/{slug}/my-work", get(views::my_work))
        .route("/workspaces/{slug}/home/widgets", get(home::widgets))
        .route(
            "/workspaces/{slug}/home/widgets/{key}",
            patch(home::set_widget),
        )
        .route(
            "/workspaces/{slug}/quick-links",
            get(home::links).post(home::add_link),
        )
        .route(
            "/workspaces/{slug}/quick-links/{id}",
            patch(home::update_link).delete(home::delete_link),
        )
        .route(
            "/workspaces/{slug}/recents",
            get(home::recents).post(home::visit),
        )
        .route(
            "/workspaces/{slug}/stickies",
            get(home::stickies).post(home::create_sticky),
        )
        .route(
            "/workspaces/{slug}/stickies/{id}",
            patch(home::update_sticky).delete(home::delete_sticky),
        )
        .route(
            "/workspaces/{slug}/work-items",
            get(views::workspace_items),
        )
        .route(
            "/workspaces/{slug}/views",
            get(views::ws_list).post(views::ws_create),
        )
        .route(
            "/workspaces/{slug}/views/{vid}",
            patch(views::ws_update).delete(views::ws_remove),
        )
        .route("/workspaces/{slug}/notifications", get(inbox::list))
        .route("/workspaces/{slug}/notifications/count", get(inbox::count))
        .route(
            "/workspaces/{slug}/notifications/read-all",
            post(inbox::read_all),
        )
        .route(
            "/workspaces/{slug}/notifications/{nid}/{action}",
            post(inbox::mark),
        )
        .route("/workspaces/{slug}/drafts", get(inbox::drafts))
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/publish",
            post(inbox::publish),
        )
        .route(
            "/me/preferences",
            get(inbox::get_prefs).patch(inbox::set_prefs),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/publish",
            get(integrations::publish_settings).post(integrations::set_publish),
        )
        .route("/public/boards/{token}", get(integrations::public_board))
        .route(
            "/public/boards/{token}/items/{seq}",
            get(integrations::public_item),
        )
        .route(
            "/public/boards/{token}/items/{seq}/comments",
            post(integrations::board_comment),
        )
        .route(
            "/public/boards/{token}/items/{seq}/comments/{cid}",
            patch(integrations::board_edit_comment).delete(integrations::board_delete_comment),
        )
        .route(
            "/public/boards/{token}/items/{seq}/reactions",
            post(integrations::board_react),
        )
        .route(
            "/public/boards/{token}/items/{seq}/votes",
            post(integrations::board_vote),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/work-items/{wid}/comments/{cid}/public",
            post(collab::set_comment_public),
        )
        .route(
            "/workspaces/{slug}/webhooks",
            get(integrations::hooks).post(integrations::create_hook),
        )
        .route(
            "/workspaces/{slug}/webhooks/{id}",
            patch(integrations::update_hook).delete(integrations::delete_hook),
        )
        .route(
            "/workspaces/{slug}/webhooks/{id}/rotate",
            post(integrations::rotate_hook),
        )
        .route(
            "/workspaces/{slug}/webhooks/{id}/ping",
            post(integrations::ping_hook),
        )
        .route(
            "/workspaces/{slug}/webhooks/{id}/deliveries",
            get(integrations::deliveries),
        )
        .route(
            "/workspaces/{slug}/webhooks/{id}/deliveries/{did}/redeliver",
            post(integrations::redeliver),
        )
        .route(
            "/workspaces/{slug}/github",
            get(integrations::github)
                .post(integrations::connect_github)
                .patch(integrations::update_github)
                .delete(integrations::disconnect_github),
        )
        .route(
            "/integrations/github/{token}",
            post(integrations::github_hook),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/export",
            get(transfer::export),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/import/preview",
            post(transfer::preview),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/import",
            post(transfer::import),
        )
        .route(
            "/workspaces/{slug}/exports",
            get(transfer::ws_exports).post(transfer::ws_export),
        )
        .route(
            "/workspaces/{slug}/exports/{id}/download",
            get(transfer::ws_export_download),
        )
        .route("/workspaces/{slug}/analytics", get(analytics::breakdown))
        .route(
            "/workspaces/{slug}/analytics/overview",
            get(analytics::overview),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/intake",
            get(intake::list).post(intake::submit),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/intake/count",
            get(intake::count),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/intake/{item}",
            get(intake::get)
                .patch(intake::edit)
                .delete(intake::withdraw),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/intake/{item}/{action}",
            post(intake::triage),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/intake-form",
            get(intake::form_settings).post(intake::set_form),
        )
        .route(
            "/public/intake/{token}",
            get(intake::public_form).post(intake::public_submit),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/pages",
            get(pages::list).post(pages::create),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/pages/{page}",
            get(pages::get).patch(pages::update).delete(pages::remove),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/pages/{page}/do/{action}",
            post(pages::flag),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/pages/{page}/duplicate",
            post(pages::duplicate),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/pages/{page}/versions",
            get(pages::versions).post(pages::save_version),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/pages/{page}/versions/{vid}",
            get(pages::version),
        )
        .route("/collab/{page}", get(pages::collab))
        .route(
            "/workspaces/{slug}/projects/{id}/modules",
            get(modules::list).post(modules::create),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/modules/{mid}",
            get(modules::get)
                .patch(modules::update)
                .delete(modules::remove),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/modules/{mid}/archive",
            post(modules::archive),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/modules/{mid}/unarchive",
            post(modules::unarchive),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/modules/{mid}/progress",
            get(modules::progress),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/modules/{mid}/links",
            get(modules::links).post(modules::add_link),
        )
        .route(
            "/workspaces/{slug}/projects/{id}/modules/{mid}/links/{lid}",
            delete(modules::remove_link),
        )
        .route("/invites/{token}", get(invites::info))
        .route("/invites/{token}/accept", post(invites::accept))
        .route("/invites/{token}/signup", post(invites::signup))
        .route(
            "/admin/settings",
            get(admin::settings).patch(admin::update_settings),
        )
        .route("/admin/auth", get(admin::auth).patch(admin::update_auth))
        .route(
            "/admin/services",
            get(admin::services).patch(admin::update_services),
        )
        .route("/workspaces/{slug}/ai", post(assist::ai))
        .route("/workspaces/{slug}/unsplash", get(assist::photos))
        .route("/admin/users", get(admin::users))
        .route("/admin/users/{id}", patch(admin::update_user))
        .route("/admin/users/{id}/reset-link", post(admin::reset_link))
        .route("/admin/workspaces", get(admin::workspaces))
        .layer(axum::extract::DefaultBodyLimit::max(
            state.max_upload + 64 * 1024,
        ))
        .fallback(system::api_not_found)
        .method_not_allowed_fallback(system::method_not_allowed);

    let app = Router::new()
        .nest("/api/v1", api)
        .route("/mcp", post(mcp::handle).get(mcp::no_stream))
        .route("/api", any(system::api_not_found))
        .route("/api/{*rest}", any(system::api_not_found))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            events::publish_changes,
        ))
        .layer(middleware::from_fn(origin::check_origin))
        .with_state(state.clone());
    let _ = state.inner.set(app.clone());
    app.merge(spa).layer(TraceLayer::new_for_http())
}
