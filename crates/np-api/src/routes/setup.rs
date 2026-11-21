//! The instance and the one-time setup wizard (SPRINT S0.2).

use crate::extract::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum_extra::extract::CookieJar;
use np_core::user::{INSTANCE_NAME_MAX, NAME_MAX};
use np_core::{Error, ErrorKind, user};
use serde::{Deserialize, Serialize};

use crate::auth::{self, ClientIp};
use crate::error::ApiResult;
use crate::{AppState, ratelimit};

#[derive(Serialize)]
pub struct InstanceInfo {
    name: String,
    setup_required: bool,
    signup_policy: np_store::instance::SignupPolicy,
    email_enabled: bool,
    /// Name for the "Sign in with …" button, when single sign-on is set up.
    sso_name: Option<String>,
    /// Email and password sign-in is offered.
    password_enabled: bool,
    /// Sign-in providers the instance admin turned on, in a fixed order.
    providers: Vec<ProviderInfo>,
    /// The AI writing assistant and Unsplash covers are set up.
    ai_enabled: bool,
    unsplash_enabled: bool,
}

#[derive(Serialize)]
pub struct ProviderInfo {
    key: &'static str,
    name: &'static str,
}

pub async fn instance(State(state): State<AppState>) -> ApiResult<Json<InstanceInfo>> {
    let i = state.store.instance().await?;
    Ok(Json(InstanceInfo {
        name: i.name,
        setup_required: !i.setup_completed,
        signup_policy: i.signup_policy,
        email_enabled: state.mailer.is_some(),
        sso_name: state.oidc.as_ref().map(|o| o.name.clone()),
        password_enabled: crate::providers::password_enabled(&state.store).await,
        providers: crate::providers::load(&state.store)
            .await?
            .into_iter()
            .filter(crate::providers::Provider::usable)
            .map(|p| ProviderInfo {
                key: p.key,
                name: p.name(),
            })
            .collect(),
        ai_enabled: crate::services::ai(&state.store).await?.enabled(),
        unsplash_enabled: !crate::services::unsplash_key(&state.store).await?.is_empty(),
    }))
}

/// Issues a fresh setup token (replacing any earlier one) and returns it. Used at startup and
/// by `nextplane setup-link`. Fails with `conflict` once setup is complete.
pub async fn issue_setup_token(store: &np_store::Store) -> np_core::Result<String> {
    let token = auth::random_token();
    store
        .set_setup_token(
            &auth::token_hash(&token),
            &auth::expires_in(auth::SETUP_TOKEN_TTL),
        )
        .await?;
    Ok(token)
}

async fn check_token(state: &AppState, token: &str) -> ApiResult<()> {
    let invalid = || {
        Error::new(
            ErrorKind::Forbidden,
            "this setup link is invalid or has expired; run `nextplane setup-link` for a new one",
        )
    };
    let (hash, expires) = state
        .store
        .setup_token()
        .await?
        .ok_or_else(|| Error::conflict("setup is already complete"))?;
    if expires <= np_store::now_ts() || auth::token_hash(token) != hash {
        return Err(invalid().into());
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct VerifyReq {
    token: String,
}

pub async fn verify(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<VerifyReq>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .limiter
        .check(&format!("setup:{ip}"), ratelimit::SETUP_PER_IP)?;
    check_token(&state, &req.token).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct SetupReq {
    token: String,
    instance_name: String,
    name: String,
    email: String,
    password: String,
}

pub async fn complete(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<SetupReq>,
) -> ApiResult<impl IntoResponse> {
    state
        .limiter
        .check(&format!("setup:{ip}"), ratelimit::SETUP_PER_IP)?;
    check_token(&state, &req.token).await?;
    let instance_name =
        user::normalize_name(&req.instance_name, INSTANCE_NAME_MAX, "instance name")?;
    if instance_name.is_empty() {
        return Err(Error::bad_request("instance name is required").into());
    }
    let name = user::normalize_name(&req.name, NAME_MAX, "name")?;
    let email = user::normalize_email(&req.email)?;
    user::validate_password(&req.password)?;
    let hash = auth::hash_password(req.password).await?;
    let user = state
        .store
        .complete_setup(&instance_name, &email, &name, &hash)
        .await?;
    tracing::info!(email = %user.email, "setup complete, instance admin created");
    let jar = auth::start_session(&state, jar, &headers, &ip, user.id).await?;
    Ok((jar, Json(user)))
}
