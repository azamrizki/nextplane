//! Self-service account endpoints: open signup, profile, password, forgot/reset.

use crate::extract::Json;
use crate::extract::Path;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum_extra::extract::CookieJar;
use np_core::user::{self, NAME_MAX};
use np_core::{Error, ErrorKind, User};
use np_store::instance::SignupPolicy;
use serde::Deserialize;
use time::Duration;

use crate::AppState;
use crate::auth::{self, ClientIp, CurrentUser};
use crate::error::ApiResult;
use crate::ratelimit;

pub const RESET_TTL_EMAIL: Duration = Duration::hours(1);
pub const RESET_TTL_ADMIN: Duration = Duration::hours(24);

#[derive(Deserialize)]
pub struct SignupReq {
    email: String,
    name: String,
    password: String,
}

/// Open signup, only when the instance's policy is `open`.
pub async fn signup(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<SignupReq>,
) -> ApiResult<impl IntoResponse> {
    state
        .limiter
        .check(&format!("signup:{ip}"), ratelimit::SIGNUP_PER_IP)?;
    if !crate::providers::password_enabled(&state.store).await {
        return Err(np_core::Error::forbidden(
            "password sign-up is turned off on this server; use a sign-in provider",
        )
        .into());
    }
    if state.store.instance().await?.signup_policy != SignupPolicy::Open {
        return Err(Error::forbidden("sign-up is by invitation only on this server").into());
    }
    let email = user::normalize_email(&req.email)?;
    let name = user::normalize_name(&req.name, NAME_MAX, "name")?;
    user::validate_password(&req.password)?;
    let hash = auth::hash_password(req.password).await?;
    let user = state
        .store
        .create_user(&email, &name, Some(&hash), false)
        .await?;
    let jar = auth::start_session(&state, jar, &headers, &ip, user.id).await?;
    Ok((StatusCode::CREATED, jar, Json(user)))
}

#[derive(Deserialize)]
pub struct ProfileReq {
    name: Option<String>,
    #[serde(default, with = "crate::util::double_option")]
    avatar: Option<Option<String>>,
}

pub async fn update_profile(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(req): Json<ProfileReq>,
) -> ApiResult<Json<User>> {
    let name = req
        .name
        .map(|n| user::normalize_name(&n, NAME_MAX, "name"))
        .transpose()?;
    if let Some(Some(a)) = &req.avatar
        && a.len() > 2048
    {
        return Err(Error::bad_request("avatar is too long").into());
    }
    let avatar = req
        .avatar
        .as_ref()
        .map(|a| a.as_deref().filter(|a| !a.trim().is_empty()));
    state
        .store
        .update_profile(me.id, name.as_deref(), avatar)
        .await?;
    Ok(Json(
        state
            .store
            .user(me.id)
            .await?
            .ok_or_else(|| Error::not_found("user not found"))?,
    ))
}

#[derive(Deserialize)]
pub struct PasswordReq {
    current: String,
    new: String,
}

/// Changes the password after checking the current one; other sessions end.
pub async fn change_password(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    headers: HeaderMap,
    Json(req): Json<PasswordReq>,
) -> ApiResult<StatusCode> {
    state
        .limiter
        .check(&format!("password:{}", me.id), ratelimit::LOGIN_PER_EMAIL)?;
    let stored = state.store.password_hash(me.id).await?;
    if !auth::verify_password(req.current, stored).await {
        return Err(Error::new(ErrorKind::BadRequest, "current password is wrong").into());
    }
    user::validate_password(&req.new)?;
    let hash = auth::hash_password(req.new).await?;
    let keep = auth::cookie_token(&headers).map(|t| auth::token_hash(&t));
    state
        .store
        .change_password(me.id, &hash, keep.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ForgotReq {
    email: String,
}

/// Emails a reset link when SMTP is set up and the account exists. Always answers 204 so
/// the response does not reveal which emails have accounts.
pub async fn forgot(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<ForgotReq>,
) -> ApiResult<StatusCode> {
    state
        .limiter
        .check(&format!("forgot-ip:{ip}"), ratelimit::RESET_PER_IP)?;
    let Ok(email) = user::normalize_email(&req.email) else {
        return Ok(StatusCode::NO_CONTENT);
    };
    if state
        .limiter
        .check(&format!("forgot-email:{email}"), ratelimit::RESET_PER_EMAIL)
        .is_err()
    {
        return Ok(StatusCode::NO_CONTENT);
    }
    let (Some(mailer), Some(user)) = (&state.mailer, state.store.user_by_email(&email).await?)
    else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let token = auth::random_token();
    state
        .store
        .create_password_reset(
            user.id,
            &auth::token_hash(&token),
            &auth::expires_in(RESET_TTL_EMAIL),
            None,
        )
        .await?;
    let link = format!("{}/reset/{token}", state.base_url(&headers));
    mailer.send_later(
        &user.email,
        "Reset your NextPlane password".into(),
        format!("Someone asked to reset the password of this NextPlane account.\n\nSet a new password (link valid 1 hour):\n{link}\n\nIf this was not you, ignore this email.\n"),
    );
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reset_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let user = state
        .store
        .password_reset_user(&auth::token_hash(&token))
        .await?
        .ok_or_else(|| Error::not_found("this reset link is invalid or has expired"))?;
    Ok(Json(serde_json::json!({ "email": user.email })))
}

#[derive(Deserialize)]
pub struct ResetReq {
    password: String,
}

/// Sets a new password from a reset link, ends all sessions and logs in.
pub async fn reset(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    jar: CookieJar,
    Path(token): Path<String>,
    Json(req): Json<ResetReq>,
) -> ApiResult<impl IntoResponse> {
    state
        .limiter
        .check(&format!("reset:{ip}"), ratelimit::RESET_PER_IP)?;
    user::validate_password(&req.password)?;
    let hash = auth::hash_password(req.password).await?;
    let user = state
        .store
        .use_password_reset(&auth::token_hash(&token), &hash)
        .await?;
    let jar = auth::start_session(&state, jar, &headers, &ip, user.id).await?;
    Ok((jar, Json(user)))
}
