//! Saved views (PERMISSIONS §2.1: contributors create; edit/delete own, admins any) and
//! "My work" across the projects a person can read.

use axum::extract::State;
use axum::http::StatusCode;
use np_core::id::Uuid;
use np_core::perm::{self, Ownership, ProjAction};
use np_core::{Error, user};
use np_store::views::{View, WorkItemInProject};
use serde::Deserialize;

use super::projects::load_project;
use super::workspaces::load;
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};

const CONFIG_MAX: usize = 32 * 1024;

fn clean(name: Option<&str>, config: Option<&serde_json::Value>) -> ApiResult<Option<String>> {
    if let Some(c) = config
        && (!c.is_object() || c.to_string().len() > CONFIG_MAX)
    {
        return Err(Error::bad_request("a view's config is a JSON object up to 32 KB").into());
    }
    let n = name
        .map(|n| user::normalize_name(n, 80, "view name"))
        .transpose()?;
    if n.as_deref() == Some("") {
        return Err(Error::bad_request("view name is required").into());
    }
    Ok(n)
}

pub async fn list(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
) -> ApiResult<Json<Vec<View>>> {
    let c = load_project(&state, &slug, pid, &me).await?;
    c.require(ProjAction::ViewWorkItems)?;
    Ok(Json(state.store.project_views(pid, me.id).await?))
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    #[serde(default)]
    is_shared: bool,
    config: serde_json::Value,
}

pub async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Json(req): Json<CreateReq>,
) -> ApiResult<(StatusCode, Json<View>)> {
    let c = load_project(&state, &slug, pid, &me).await?;
    c.require_write(ProjAction::CreateView)?;
    let name = clean(Some(&req.name), Some(&req.config))?.unwrap_or_default();
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_view(
                    c.ws.workspace.id,
                    pid,
                    me.id,
                    &name,
                    req.is_shared,
                    &req.config,
                )
                .await?,
        ),
    ))
}

async fn editable(
    state: &AppState,
    me: &np_core::User,
    slug: &str,
    pid: Uuid,
    vid: Uuid,
) -> ApiResult<View> {
    let c = load_project(state, slug, pid, me).await?;
    let v = state
        .store
        .view(pid, vid)
        .await?
        .filter(|v| v.is_shared || v.owner_id == me.id)
        .ok_or_else(|| Error::not_found("view not found"))?;
    let own = if v.owner_id == me.id {
        Ownership::Own
    } else {
        Ownership::Others
    };
    c.require_write(ProjAction::EditView(own))?;
    Ok(v)
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    is_shared: Option<bool>,
    config: Option<serde_json::Value>,
}

pub async fn update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, vid)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<UpdateReq>,
) -> ApiResult<Json<View>> {
    let v = editable(&state, &me, &slug, pid, vid).await?;
    // Only the owner decides whether a personal view is shared.
    if req.is_shared.is_some() && v.owner_id != me.id {
        return Err(Error::forbidden("only the owner can share or unshare a view").into());
    }
    let name = clean(req.name.as_deref(), req.config.as_ref())?;
    Ok(Json(
        state
            .store
            .update_view(
                pid,
                vid,
                name.as_deref(),
                req.is_shared,
                req.config.as_ref(),
            )
            .await?,
    ))
}

pub async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, vid)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    editable(&state, &me, &slug, pid, vid).await?;
    state.store.delete_view(vid).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct MyWorkQuery {
    #[serde(default = "assigned")]
    kind: String,
    #[serde(default)]
    include_closed: bool,
}

fn assigned() -> String {
    "assigned".into()
}

pub async fn my_work(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Query(q): Query<MyWorkQuery>,
) -> ApiResult<Json<Vec<WorkItemInProject>>> {
    let ws = load(&state, &slug, &me).await?;
    let rows = state
        .store
        .workspace_projects(ws.workspace.id, me.id, false)
        .await?;
    let readable: Vec<(Uuid, String, String)> = rows
        .into_iter()
        .filter(|r| {
            perm::effective_proj_role(ws.role, r.member_role.0)
                .is_some_and(|role| perm::proj_can(role, ProjAction::ViewWorkItems))
                && (r.member_role.0.is_some() || ws.role.is_admin())
        })
        .map(|r| (r.project.id, r.project.identifier, r.project.name))
        .collect();
    Ok(Json(
        state
            .store
            .my_work(&readable, me.id, &q.kind, q.include_closed)
            .await?,
    ))
}
