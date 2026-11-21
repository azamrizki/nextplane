//! Login, logout and the current user.

use crate::extract::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum_extra::extract::CookieJar;
use np_core::{Error, ErrorKind, User, user};
use serde::Deserialize;

use crate::auth::{self, ClientIp, CurrentUser};
use crate::error::ApiResult;
use crate::{AppState, ratelimit};

#[derive(Deserialize)]
pub struct LoginReq {
    email: String,
    password: String,
}

pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<LoginReq>,
) -> ApiResult<impl IntoResponse> {
    let bad = || Error::new(ErrorKind::Unauthorized, "wrong email or password");
    state
        .limiter
        .check(&format!("login-ip:{ip}"), ratelimit::LOGIN_PER_IP)?;
    let email = user::normalize_email(&req.email).map_err(|_| bad())?;
    let email_key = format!("login-email:{email}");
    state
        .limiter
        .check(&email_key, ratelimit::LOGIN_PER_EMAIL)?;

    let found = state.store.user_for_login(&email).await?;
    let (user, hash) = match found {
        Some((u, h)) => (Some(u), h),
        None => (None, None),
    };
    if !auth::verify_password(req.password, hash).await {
        return Err(bad().into());
    }
    let user = user.ok_or_else(bad)?;
    // With password sign-in turned off, only instance admins may still use one: the way back
    // in when a sign-in provider breaks.
    if !user.is_instance_admin && !crate::providers::password_enabled(&state.store).await {
        return Err(Error::forbidden(
            "password sign-in is turned off on this server; use a sign-in provider",
        )
        .into());
    }
    state.limiter.reset(&email_key);
    let jar = auth::start_session(&state, jar, &headers, &ip, user.id).await?;
    Ok((jar, Json(user)))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> ApiResult<impl IntoResponse> {
    if let Some(token) = auth::cookie_token(&headers) {
        state
            .store
            .delete_session_by_hash(&auth::token_hash(&token))
            .await?;
    }
    let jar = jar.add(auth::clear_session_cookie(auth::is_https(&headers)));
    Ok((jar, StatusCode::NO_CONTENT))
}

pub async fn me(CurrentUser(user): CurrentUser) -> Json<User> {
    Json(user)
}
