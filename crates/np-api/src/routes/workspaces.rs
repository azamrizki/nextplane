//! Workspaces: create, list, settings, trash and restore.

use crate::extract::Json;
use crate::extract::{Path, Query};
use axum::extract::State;
use axum::http::StatusCode;
use np_core::perm::{WsAction, WsFacts, ws_require};
use np_core::{Error, user, workspace};
use np_store::workspaces::{MyWorkspace, Workspace, WorkspacePatch};
use serde::Deserialize;

use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;

pub fn facts(ws: &Workspace) -> WsFacts {
    WsFacts {
        members_can_create_projects: ws.members_can_create_projects,
    }
}

/// The workspace with this slug and the caller's role, or `404`.
pub async fn load(state: &AppState, slug: &str, user: &np_core::User) -> ApiResult<MyWorkspace> {
    Ok(state.store.workspace_for(slug, user.id).await?)
}

pub async fn list(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<Vec<MyWorkspace>>> {
    Ok(Json(state.store.my_workspaces(me.id).await?))
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    slug: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(req): Json<CreateReq>,
) -> ApiResult<(StatusCode, Json<MyWorkspace>)> {
    let name = user::normalize_name(&req.name, workspace::NAME_MAX, "workspace name")?;
    if name.is_empty() {
        return Err(Error::bad_request("workspace name is required").into());
    }
    let slug = workspace::validate_slug(req.slug.as_deref().unwrap_or(&workspace::slugify(&name)))?;
    let ws = state.store.create_workspace(&slug, &name, me.id).await?;
    Ok((StatusCode::CREATED, Json(ws)))
}

#[derive(Deserialize)]
pub struct SlugQuery {
    slug: String,
}

/// Whether a slug is valid and free, for the create form.
pub async fn slug_check(
    State(state): State<AppState>,
    CurrentUser(_): CurrentUser,
    Query(q): Query<SlugQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let slug = match workspace::validate_slug(&q.slug) {
        Ok(s) => s,
        Err(e) => {
            return Ok(Json(
                serde_json::json!({ "available": false, "reason": e.message }),
            ));
        }
    };
    let taken: i64 = sqlx_taken(&state, &slug).await?;
    Ok(Json(
        serde_json::json!({ "available": taken == 0, "slug": slug,
        "reason": if taken > 0 { Some("this workspace URL is taken") } else { None } }),
    ))
}

async fn sqlx_taken(state: &AppState, slug: &str) -> ApiResult<i64> {
    Ok(state.store.slug_count(slug).await?)
}

pub async fn get(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<MyWorkspace>> {
    Ok(Json(load(&state, &slug, &me).await?))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    slug: Option<String>,
    #[serde(default, with = "crate::util::double_option")]
    logo: Option<Option<String>>,
    members_can_create_projects: Option<bool>,
}

pub async fn update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<UpdateReq>,
) -> ApiResult<Json<MyWorkspace>> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::EditSettings, facts(&ws.workspace))?;
    let mut p = WorkspacePatch::default();
    if let Some(n) = req.name {
        let n = user::normalize_name(&n, workspace::NAME_MAX, "workspace name")?;
        if n.is_empty() {
            return Err(Error::bad_request("workspace name is required").into());
        }
        p.name = Some(n);
    }
    if let Some(s) = req.slug {
        p.slug = Some(workspace::validate_slug(&s)?);
    }
    if let Some(logo) = req.logo {
        if let Some(l) = &logo
            && l.len() > 2048
        {
            return Err(Error::bad_request("logo is too long").into());
        }
        p.logo = Some(logo.filter(|l| !l.trim().is_empty()));
    }
    p.members_can_create_projects = req.members_can_create_projects;
    state.store.update_workspace(ws.workspace.id, &p).await?;
    let slug = p.slug.unwrap_or(slug);
    Ok(Json(load(&state, &slug, &me).await?))
}

pub async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<StatusCode> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::Delete, facts(&ws.workspace))?;
    state.store.trash_workspace(ws.workspace.id).await?;
    tracing::info!(workspace = %ws.workspace.slug, by = %me.email, "workspace moved to trash");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn trash(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<Vec<Workspace>>> {
    Ok(Json(state.store.trashed_workspaces(me.id).await?))
}

pub async fn restore(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<MyWorkspace>> {
    state.store.restore_workspace(&slug, me.id).await?;
    Ok(Json(load(&state, &slug, &me).await?))
}
