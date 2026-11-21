//! Saved views (PERMISSIONS §2.1: contributors create; edit/delete own, admins any) and
//! "My work" across the projects a person can read.

use axum::extract::State;
use axum::http::StatusCode;
use np_core::id::Uuid;
use np_core::perm::{self, Ownership, ProjAction};
use np_core::{Error, user};
use np_store::views::{View, WorkItemInProject};
use serde::Deserialize;

use std::collections::HashMap;

use np_core::perm::WsRole;

use super::projects::load_project;
use super::work_items::{ListQuery, PAGE_DEFAULT, PAGE_MAX, filter_from, ids};
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

// ---- Workspace views (S3.7, as in Plane) ---------------------------------------------------

fn encode_ws_cursor(at: &str, id: Uuid) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{at}|{}", id.simple()))
}

fn decode_ws_cursor(c: &str) -> ApiResult<(String, Uuid)> {
    use base64::Engine;
    let bad = || Error::bad_request("bad cursor");
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(c)
        .map_err(|_| bad())?;
    let s = String::from_utf8(raw).map_err(|_| bad())?;
    let (at, id) = s.split_once('|').ok_or_else(bad)?;
    Ok((at.to_string(), id.parse().map_err(|_| bad())?))
}

/// `GET /workspaces/{slug}/work-items`: published, unarchived work items of the live projects
/// the caller is a member of and may read, newest first, with the project filters plus
/// `project` and `subscribed=true`. Pages of `limit` (default 1000); the next page's cursor is
/// in `X-Next-Cursor`.
pub async fn workspace_items(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Query(q): Query<ListQuery>,
) -> ApiResult<impl axum::response::IntoResponse> {
    let ws = load(&state, &slug, &me).await?;
    if q.archived {
        return Err(Error::bad_request("archived work items are listed per project").into());
    }
    let wanted = ids(&q.project)?;
    let rows = state
        .store
        .workspace_projects(ws.workspace.id, me.id, false)
        .await?;
    let readable: HashMap<Uuid, (String, String)> = rows
        .into_iter()
        .filter(|r| {
            r.member_role.0.is_some()
                && perm::effective_proj_role(ws.role, r.member_role.0)
                    .is_some_and(|role| perm::proj_can(role, ProjAction::ViewWorkItems))
                && (wanted.is_empty() || wanted.contains(&r.project.id))
        })
        .map(|r| (r.project.id, (r.project.identifier, r.project.name)))
        .collect();
    let limit = q.limit.unwrap_or(PAGE_DEFAULT).clamp(1, PAGE_MAX);
    let mut f = filter_from(&q)?;
    f.newest_first = true;
    f.limit = Some(limit);
    f.before = q.cursor.as_deref().map(decode_ws_cursor).transpose()?;
    if q.subscribed {
        f.subscribed_by = Some(me.id);
    }
    let pids: Vec<Uuid> = readable.keys().copied().collect();
    let items = state.store.work_items_in(&pids, &f).await?;
    let mut headers = axum::http::HeaderMap::new();
    if items.len() as i64 == limit
        && let Some(last) = items.last()
        && let Ok(v) = encode_ws_cursor(&last.created_at, last.id).parse()
    {
        headers.insert("x-next-cursor", v);
    }
    let out: Vec<WorkItemInProject> = items
        .into_iter()
        .map(|item| {
            let (identifier, project_name) = readable.get(&item.project_id).cloned().unwrap_or_default();
            WorkItemInProject {
                item,
                identifier,
                project_name,
            }
        })
        .collect();
    Ok((headers, Json(out)))
}

/// Workspace views: shared ones and your own; workspace guests see only their own.
pub async fn ws_list(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<View>>> {
    let ws = load(&state, &slug, &me).await?;
    Ok(Json(
        state
            .store
            .workspace_views(ws.workspace.id, me.id, ws.role == WsRole::Guest)
            .await?,
    ))
}

#[derive(Deserialize)]
pub struct WsCreateReq {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "yes")]
    is_shared: bool,
    config: serde_json::Value,
}

fn yes() -> bool {
    true
}

fn clean_description(d: &str) -> String {
    d.trim().chars().take(500).collect()
}

pub async fn ws_create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<WsCreateReq>,
) -> ApiResult<(StatusCode, Json<View>)> {
    let ws = load(&state, &slug, &me).await?;
    let name = clean(Some(&req.name), Some(&req.config))?.unwrap_or_default();
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_workspace_view(
                    ws.workspace.id,
                    me.id,
                    &name,
                    &clean_description(&req.description),
                    req.is_shared,
                    &req.config,
                )
                .await?,
        ),
    ))
}

#[derive(Deserialize)]
pub struct WsUpdateReq {
    name: Option<String>,
    description: Option<String>,
    is_shared: Option<bool>,
    config: Option<serde_json::Value>,
}

/// A workspace view the caller can see, with the workspace.
async fn ws_view(
    state: &AppState,
    me: &np_core::User,
    slug: &str,
    vid: Uuid,
) -> ApiResult<(np_store::workspaces::MyWorkspace, View)> {
    let ws = load(state, slug, me).await?;
    let v = state
        .store
        .workspace_view(ws.workspace.id, vid)
        .await?
        .filter(|v| v.owner_id == me.id || (v.is_shared && ws.role != WsRole::Guest))
        .ok_or_else(|| Error::not_found("view not found"))?;
    Ok((ws, v))
}

/// Only the owner edits a workspace view (as in Plane).
pub async fn ws_update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, vid)): Path<(String, Uuid)>,
    Json(req): Json<WsUpdateReq>,
) -> ApiResult<Json<View>> {
    let (ws, v) = ws_view(&state, &me, &slug, vid).await?;
    if v.owner_id != me.id {
        return Err(Error::forbidden("only the view's owner can change it").into());
    }
    let name = clean(req.name.as_deref(), req.config.as_ref())?;
    let description = req.description.as_deref().map(clean_description);
    Ok(Json(
        state
            .store
            .update_workspace_view(
                ws.workspace.id,
                vid,
                name.as_deref(),
                description.as_deref(),
                req.is_shared,
                req.config.as_ref(),
            )
            .await?,
    ))
}

/// The owner or a workspace admin deletes a workspace view.
pub async fn ws_remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, vid)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    let (ws, v) = ws_view(&state, &me, &slug, vid).await?;
    if v.owner_id != me.id && !ws.role.is_admin() {
        return Err(Error::forbidden("only the view's owner or a workspace admin can delete it").into());
    }
    state.store.delete_view(vid).await?;
    Ok(StatusCode::NO_CONTENT)
}
