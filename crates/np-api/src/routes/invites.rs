//! Opening and accepting an invite link (no workspace membership needed).

use crate::extract::Json;
use crate::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum_extra::extract::CookieJar;
use np_core::user::{self, NAME_MAX};
use np_core::{Error, ErrorKind};
use np_store::instance::SignupPolicy;
use np_store::invites::InviteInfo;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::auth::{self, ClientIp, CurrentUser};
use crate::error::ApiResult;
use crate::ratelimit;

async fn find(state: &AppState, token: &str) -> ApiResult<InviteInfo> {
    Ok(state
        .store
        .invite_by_token(&auth::token_hash(token))
        .await?
        .ok_or_else(|| Error::not_found("this invite is invalid, used or expired"))?)
}

#[derive(Serialize)]
pub struct InfoRes {
    #[serde(flatten)]
    invite: InviteInfo,
    /// Whether this server lets the invite create a new account.
    signup_allowed: bool,
}

pub async fn info(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Path(token): Path<String>,
) -> ApiResult<Json<InfoRes>> {
    state
        .limiter
        .check(&format!("invite-open:{ip}"), ratelimit::RESET_PER_IP)?;
    let invite = find(&state, &token).await?;
    if !crate::providers::password_enabled(&state.store).await {
        return Err(np_core::Error::forbidden(
            "password sign-up is turned off on this server; use a sign-in provider",
        )
        .into());
    }
    let policy = state.store.instance().await?.signup_policy;
    Ok(Json(InfoRes {
        invite,
        signup_allowed: policy != SignupPolicy::Closed,
    }))
}

/// Accepts the invite as the logged-in user, whose email must match the invite.
pub async fn accept(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(token): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invite = find(&state, &token).await?;
    if invite.email != me.email {
        return Err(Error::forbidden(format!(
            "this invite was sent to {}; log in with that account to accept it",
            invite.email
        ))
        .into());
    }
    state.store.accept_invite(&invite, me.id).await?;
    Ok(Json(
        serde_json::json!({ "workspace_slug": invite.workspace_slug }),
    ))
}

#[derive(Deserialize)]
pub struct SignupReq {
    name: String,
    password: String,
}

/// Creates the invited person's account, accepts the invite and logs them in.
pub async fn signup(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    jar: CookieJar,
    Path(token): Path<String>,
    Json(req): Json<SignupReq>,
) -> ApiResult<impl IntoResponse> {
    state
        .limiter
        .check(&format!("signup:{ip}"), ratelimit::SIGNUP_PER_IP)?;
    let invite = find(&state, &token).await?;
    if state.store.instance().await?.signup_policy == SignupPolicy::Closed {
        return Err(Error::forbidden("new accounts are turned off on this server").into());
    }
    if invite.account_exists {
        return Err(Error::new(
            ErrorKind::Conflict,
            "an account with this email exists; log in to accept",
        )
        .into());
    }
    let name = user::normalize_name(&req.name, NAME_MAX, "name")?;
    user::validate_password(&req.password)?;
    let hash = auth::hash_password(req.password).await?;
    let user = state
        .store
        .accept_invite_with_signup(&invite, &name, &hash)
        .await?;
    let jar = auth::start_session(&state, jar, &headers, &ip, user.id).await?;
    Ok((
        jar,
        Json(serde_json::json!({ "user": user, "workspace_slug": invite.workspace_slug })),
    ))
}
