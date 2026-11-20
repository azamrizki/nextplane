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
