//! Instance admin: settings, users, workspaces (PERMISSIONS §3). Instance admins get no
//! access to workspace content from this.

use crate::extract::Json;
use crate::extract::Path;
use axum::extract::{FromRequestParts, State};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use np_core::id::Uuid;
use np_core::user::{self, INSTANCE_NAME_MAX};
use np_core::{Error, User};
use np_store::instance::SignupPolicy;
use serde::{Deserialize, Serialize};

use super::account::RESET_TTL_ADMIN;
use crate::AppState;
use crate::providers::Provider;
use crate::auth::{self, CurrentUser};
use crate::error::{ApiError, ApiResult};

/// The logged-in user, who must be an instance admin.
pub struct InstanceAdmin(pub User);

impl FromRequestParts<AppState> for InstanceAdmin {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let CurrentUser(u) = CurrentUser::from_request_parts(parts, state).await?;
        if !u.is_instance_admin {
            return Err(Error::forbidden("instance admins only").into());
        }
        Ok(InstanceAdmin(u))
    }
}

#[derive(Serialize)]
pub struct Settings {
    name: String,
    signup_policy: SignupPolicy,
    email_enabled: bool,
    public_url: Option<String>,
}

pub async fn settings(
    State(state): State<AppState>,
    InstanceAdmin(_): InstanceAdmin,
) -> ApiResult<Json<Settings>> {
    current_settings(&state).await
}

async fn current_settings(state: &AppState) -> ApiResult<Json<Settings>> {
    let i = state.store.instance().await?;
    Ok(Json(Settings {
        name: i.name,
        signup_policy: i.signup_policy,
        email_enabled: state.mailer.is_some(),
        public_url: state.public_url.clone(),
    }))
}

#[derive(Deserialize)]
pub struct SettingsReq {
    name: Option<String>,
    signup_policy: Option<SignupPolicy>,
}

pub async fn update_settings(
    State(state): State<AppState>,
    InstanceAdmin(_): InstanceAdmin,
    Json(req): Json<SettingsReq>,
) -> ApiResult<Json<Settings>> {
    let name = req
        .name
        .map(|n| user::normalize_name(&n, INSTANCE_NAME_MAX, "instance name"))
        .transpose()?;
    if name.as_deref() == Some("") {
        return Err(Error::bad_request("instance name is required").into());
    }
    state
        .store
        .update_instance(name.as_deref(), req.signup_policy)
        .await?;
    current_settings(&state).await
}

pub async fn users(
    State(state): State<AppState>,
    InstanceAdmin(_): InstanceAdmin,
) -> ApiResult<Json<Vec<User>>> {
    Ok(Json(state.store.all_users().await?))
}

#[derive(Deserialize)]
pub struct UserReq {
    is_instance_admin: Option<bool>,
}

pub async fn update_user(
    State(state): State<AppState>,
    InstanceAdmin(_): InstanceAdmin,
    Path(id): Path<Uuid>,
    Json(req): Json<UserReq>,
) -> ApiResult<Json<User>> {
    let target = state
        .store
        .user(id)
        .await?
        .ok_or_else(|| Error::not_found("user not found"))?;
    if let Some(on) = req.is_instance_admin {
        if !on && target.is_instance_admin && state.store.count_instance_admins().await? <= 1 {
            return Err(
                Error::conflict("the instance must keep at least one instance admin").into(),
            );
        }
        state.store.set_instance_admin(id, on).await?;
    }
    Ok(Json(
        state
            .store
            .user(id)
            .await?
            .ok_or_else(|| Error::not_found("user not found"))?,
    ))
}

/// A password-reset link for a user, valid 24 hours, for the admin to pass on.
pub async fn reset_link(
    State(state): State<AppState>,
    InstanceAdmin(admin): InstanceAdmin,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let target = state
        .store
        .user(id)
        .await?
        .ok_or_else(|| Error::not_found("user not found"))?;
    let token = auth::random_token();
    state
        .store
        .create_password_reset(
            target.id,
            &auth::token_hash(&token),
            &auth::expires_in(RESET_TTL_ADMIN),
            Some(admin.id),
        )
        .await?;
    tracing::info!(user = %target.email, by = %admin.email, "password reset link created");
    Ok(Json(
        serde_json::json!({ "link": format!("{}/reset/{token}", state.base_url(&headers)), "expires_in_hours": 24 }),
    ))
}

#[derive(Serialize)]
pub struct AdminWorkspace {
    #[serde(flatten)]
    workspace: np_store::workspaces::Workspace,
    member_count: i64,
}

pub async fn workspaces(
    State(state): State<AppState>,
    InstanceAdmin(_): InstanceAdmin,
) -> ApiResult<Json<Vec<AdminWorkspace>>> {
    Ok(Json(
        state
            .store
            .all_workspaces()
            .await?
            .into_iter()
            .map(|(workspace, member_count)| AdminWorkspace {
                workspace,
                member_count,
            })
            .collect(),
    ))
}

// ---- Sign-in methods (S3.7, as in Plane's "Authentication" page) ----------------------------

#[derive(Serialize)]
pub struct ProviderOut {
    key: &'static str,
    name: &'static str,
    enabled: bool,
    configured: bool,
    client_id: String,
    /// Whether a client secret is saved; the secret itself is never sent back.
    has_secret: bool,
    host: String,
    org: String,
    /// Where to send people back after they sign in: register it with the provider.
    callback_url: String,
}

#[derive(Serialize)]
pub struct AuthSettings {
    password_enabled: bool,
    signup_policy: SignupPolicy,
    /// The OpenID provider of config.toml, if any (managed in that file).
    config_sso: Option<String>,
    providers: Vec<ProviderOut>,
}

async fn auth_settings(state: &AppState, headers: &HeaderMap) -> ApiResult<Json<AuthSettings>> {
    let base = state.base_url(headers);
    let list = crate::providers::load(&state.store).await?;
    Ok(Json(AuthSettings {
        password_enabled: crate::providers::password_enabled(&state.store).await,
        signup_policy: state.store.instance().await?.signup_policy,
        config_sso: state.oidc.as_ref().map(|o| o.name.clone()),
        providers: list
            .into_iter()
            .map(|p| ProviderOut {
                key: p.key,
                name: p.name(),
                enabled: p.enabled,
                configured: p.configured(),
                client_id: p.client_id.clone(),
                has_secret: !p.client_secret.is_empty(),
                host: p.host.clone(),
                org: p.org.clone(),
                callback_url: format!("{base}/api/v1/auth/oauth/{}/callback", p.key),
            })
            .collect(),
    }))
}

pub async fn auth(
    State(state): State<AppState>,
    InstanceAdmin(_): InstanceAdmin,
    headers: HeaderMap,
) -> ApiResult<Json<AuthSettings>> {
    auth_settings(&state, &headers).await
}

#[derive(Deserialize)]
pub struct ProviderReq {
    enabled: Option<bool>,
    client_id: Option<String>,
    /// An empty string keeps the saved secret.
    client_secret: Option<String>,
    host: Option<String>,
    org: Option<String>,
}

#[derive(Deserialize)]
pub struct AuthReq {
    password_enabled: Option<bool>,
    #[serde(default)]
    providers: std::collections::HashMap<String, ProviderReq>,
}

fn clean_host(h: &str) -> ApiResult<String> {
    let h = h.trim().trim_end_matches('/');
    if h.is_empty() {
        return Ok(String::new());
    }
    let ok = (h.starts_with("https://") || h.starts_with("http://"))
        && reqwest::Url::parse(h).is_ok_and(|u| u.host_str().is_some_and(|x| !x.is_empty()));
    if !ok {
        return Err(Error::bad_request("the host is a web address, like https://gitlab.com").into());
    }
    Ok(h.to_string())
}

/// Changes sign-in methods. At least one way to sign in must stay on (as in Plane, but also
/// checked here, not only in the browser).
pub async fn update_auth(
    State(state): State<AppState>,
    InstanceAdmin(admin): InstanceAdmin,
    headers: HeaderMap,
    Json(req): Json<AuthReq>,
) -> ApiResult<Json<AuthSettings>> {
    let mut list = crate::providers::load(&state.store).await?;
    let mut entries: Vec<(String, Option<String>)> = Vec::new();
    for (key, r) in &req.providers {
        let Some(p) = list.iter_mut().find(|p| p.key == key.as_str()) else {
            return Err(Error::bad_request(format!("unknown sign-in provider {key:?}")).into());
        };
        let k = |f: &str| format!("auth.{}.{f}", p.key);
        if let Some(v) = &r.client_id {
            p.client_id = v.trim().chars().take(500).collect();
            entries.push((k("client_id"), Some(p.client_id.clone())));
        }
        if let Some(v) = r.client_secret.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            p.client_secret = v.chars().take(500).collect();
            entries.push((k("client_secret"), Some(p.client_secret.clone())));
        }
        if let Some(v) = &r.host {
            p.host = clean_host(v)?;
            entries.push((k("host"), Some(p.host.clone())));
        }
        if let Some(v) = &r.org {
            let org = v.trim();
            if !org.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(Error::bad_request("a GitHub organization is its short name, like acme-inc").into());
            }
            p.org = org.to_string();
            entries.push((k("org"), Some(p.org.clone())));
        }
        if let Some(on) = r.enabled {
            if on && !p.configured() {
                return Err(Error::bad_request(format!(
                    "set up {} (client id, secret{}) before turning it on",
                    p.name(),
                    if matches!(p.key, "gitlab" | "gitea") { " and host" } else { "" }
                ))
                .into());
            }
            p.enabled = on;
            entries.push((k("enabled"), Some(if on { "1" } else { "0" }.into())));
        }
    }
    let password = req
        .password_enabled
        .unwrap_or(crate::providers::password_enabled(&state.store).await);
    if !password && state.oidc.is_none() && !list.iter().any(Provider::usable) {
        return Err(Error::conflict("at least one way to sign in must stay on").into());
    }
    if let Some(on) = req.password_enabled {
        entries.push(("auth.password_enabled".into(), Some(if on { "1" } else { "0" }.into())));
    }
    state.store.set_config(&entries).await?;
    tracing::info!(by = %admin.email, "sign-in settings changed");
    auth_settings(&state, &headers).await
}

// ---- AI assistant and Unsplash (S3.7, as in Plane's "AI" and "Images" pages) ---------------

#[derive(Serialize)]
pub struct AiOut {
    enabled: bool,
    has_key: bool,
    model: String,
    base_url: String,
}

#[derive(Serialize)]
pub struct ServicesOut {
    ai: AiOut,
    unsplash: UnsplashOut,
}

#[derive(Serialize)]
pub struct UnsplashOut {
    enabled: bool,
}

async fn services_out(state: &AppState) -> ApiResult<Json<ServicesOut>> {
    let ai = crate::services::ai(&state.store).await?;
    let key = crate::services::unsplash_key(&state.store).await?;
    Ok(Json(ServicesOut {
        ai: AiOut {
            enabled: ai.enabled(),
            has_key: ai.enabled(),
            model: ai.model,
            base_url: ai.base_url,
        },
        unsplash: UnsplashOut {
            enabled: !key.is_empty(),
        },
    }))
}

pub async fn services(
    State(state): State<AppState>,
    InstanceAdmin(_): InstanceAdmin,
) -> ApiResult<Json<ServicesOut>> {
    services_out(&state).await
}

#[derive(Deserialize, Default)]
pub struct AiReq {
    /// An empty string removes the key.
    api_key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct UnsplashReq {
    /// An empty string removes the key.
    access_key: Option<String>,
}

#[derive(Deserialize)]
pub struct ServicesReq {
    #[serde(default)]
    ai: Option<AiReq>,
    #[serde(default)]
    unsplash: Option<UnsplashReq>,
}

/// Saves keys and settings; keys are never sent back, only whether one is set.
pub async fn update_services(
    State(state): State<AppState>,
    InstanceAdmin(admin): InstanceAdmin,
    Json(req): Json<ServicesReq>,
) -> ApiResult<Json<ServicesOut>> {
    let mut entries: Vec<(String, Option<String>)> = Vec::new();
    let opt = |v: &str| {
        let v = v.trim();
        (!v.is_empty()).then(|| v.chars().take(500).collect::<String>())
    };
    if let Some(ai) = req.ai {
        if let Some(k) = ai.api_key {
            entries.push(("ai.api_key".into(), opt(&k)));
        }
        if let Some(m) = ai.model {
            entries.push(("ai.model".into(), opt(&m)));
        }
        if let Some(b) = ai.base_url {
            let b = b.trim().trim_end_matches('/').to_string();
            if !b.is_empty()
                && !(b.starts_with("https://") || b.starts_with("http://"))
            {
                return Err(Error::bad_request("the AI address is a web address, like https://api.openai.com/v1").into());
            }
            entries.push(("ai.base_url".into(), opt(&b)));
        }
    }
    if let Some(u) = req.unsplash
        && let Some(k) = u.access_key
    {
        entries.push(("unsplash.access_key".into(), opt(&k)));
    }
    state.store.set_config(&entries).await?;
    tracing::info!(by = %admin.email, "outside services changed");
    services_out(&state).await
}
