//! Modules (SPRINT S2.2). Same rules as cycles: contributors create and edit, delete their
//! own; admins any (PERMISSIONS §2.1).

use axum::extract::State;
use axum::http::StatusCode;
use np_core::id::Uuid;
use np_core::perm::{Ownership, ProjAction};
use np_core::project::EstimateType;
use np_core::{Error, User, user, work_item};
use np_store::cycles::Progress;
use np_store::modules::{Module, ModuleFields, ModuleLink, STATUSES};
use serde::Deserialize;

use super::projects::{ProjCtx, load_project};
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};

async fn ctx(state: &AppState, me: &User, slug: &str, pid: Uuid) -> ApiResult<ProjCtx> {
    let c = load_project(state, slug, pid, me).await?;
    c.require(ProjAction::ViewWorkItems)?;
    if !c.row.project.feature_modules {
        return Err(Error::forbidden("modules are turned off for this project").into());
    }
    Ok(c)
}

async fn load(state: &AppState, pid: Uuid, id: Uuid) -> ApiResult<Module> {
    Ok(state
        .store
        .module(pid, id)
        .await?
        .ok_or_else(|| Error::not_found("module not found"))?)
}

/// Archived modules are read-only until restored (as in Plane).
fn live(m: Module) -> ApiResult<Module> {
    if m.archived_at.is_some() {
        return Err(Error::conflict("this module is archived; restore it to make changes").into());
    }
    Ok(m)
}

#[derive(Deserialize, Default)]
pub struct Fields {
    name: Option<String>,
    description: Option<String>,
    status: Option<String>,
    #[serde(default, with = "crate::util::double_option")]
    lead_id: Option<Option<Uuid>>,
    #[serde(default, with = "crate::util::double_option")]
    start_date: Option<Option<String>>,
    #[serde(default, with = "crate::util::double_option")]
    target_date: Option<Option<String>>,
    member_ids: Option<Vec<Uuid>>,
}

async fn clean(
    state: &AppState,
    c: &ProjCtx,
    f: Fields,
    cur: Option<&Module>,
) -> ApiResult<ModuleFields> {
    let name = f
        .name
        .as_deref()
        .map(|n| user::normalize_name(n, 80, "module name"))
        .transpose()?;
    if name.as_deref() == Some("") || (cur.is_none() && name.is_none()) {
        return Err(Error::bad_request("module name is required").into());
    }
    if let Some(s) = &f.status
        && !STATUSES.contains(&s.as_str())
    {
        return Err(Error::bad_request(format!("status is one of {}", STATUSES.join(", "))).into());
    }
    let dates = if f.start_date.is_some() || f.target_date.is_some() || cur.is_none() {
        let d = |x: Option<String>| {
            x.filter(|v| !v.is_empty())
                .map(|v| work_item::validate_date(&v))
                .transpose()
        };
        let s = d(f
            .start_date
            .unwrap_or_else(|| cur.and_then(|m| m.start_date.clone())))?;
        let e = d(f
            .target_date
            .unwrap_or_else(|| cur.and_then(|m| m.target_date.clone())))?;
        work_item::check_date_order(s.as_deref(), e.as_deref())?;
        Some((s, e))
    } else {
        None
    };
    let ws = c.ws.workspace.id;
    for u in f
        .lead_id
        .flatten()
        .iter()
        .chain(f.member_ids.iter().flatten())
    {
        if state.store.ws_role(ws, *u).await?.is_none() {
            return Err(Error::bad_request("leads and members must be in this workspace").into());
        }
    }
    Ok(ModuleFields {
        name,
        description: f.description.map(|d| d.trim().chars().take(4000).collect()),
        status: f.status,
        lead_id: f.lead_id,
        dates,
        member_ids: f.member_ids,
    })
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    archived: bool,
}

pub async fn list(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Module>>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    if q.archived {
        c.require(ProjAction::ViewArchives)?;
    }
    Ok(Json(state.store.modules(pid, q.archived).await?))
}

pub async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Json(f): Json<Fields>,
) -> ApiResult<(StatusCode, Json<Module>)> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::CreateCycleOrModule)?;
    let mut f = clean(&state, &c, f, None).await?;
    if f.lead_id.is_none() {
        f.lead_id = Some(Some(me.id));
    }
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_module(pid, &f, me.id).await?),
    ))
}

pub async fn get(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Module>> {
    ctx(&state, &me, &slug, pid).await?;
    Ok(Json(load(&state, pid, id).await?))
}

pub async fn update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
    Json(f): Json<Fields>,
) -> ApiResult<Json<Module>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::EditCycleOrModule)?;
    let cur = live(load(&state, pid, id).await?)?;
    let f = clean(&state, &c, f, Some(&cur)).await?;
    Ok(Json(state.store.update_module(pid, id, &f).await?))
}

pub async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let m = live(load(&state, pid, id).await?)?;
    let own = if m.created_by == me.id {
        Ownership::Own
    } else {
        Ownership::Others
    };
    c.require_write(ProjAction::DeleteCycleOrModule(own))?;
    state.store.delete_module(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_archived(
    state: AppState,
    me: User,
    slug: String,
    pid: Uuid,
    id: Uuid,
    on: bool,
) -> ApiResult<Json<Module>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::EditCycleOrModule)?;
    let m = load(&state, pid, id).await?;
    if on && m.archived_at.is_none() && !matches!(m.status.as_str(), "completed" | "cancelled") {
        return Err(Error::bad_request(
            "only completed or cancelled modules can be archived",
        )
        .into());
    }
    state.store.set_module_archived(id, on).await?;
    Ok(Json(load(&state, pid, id).await?))
}

pub async fn archive(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Module>> {
    set_archived(state, me, slug, pid, id, true).await
}

pub async fn unarchive(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Module>> {
    set_archived(state, me, slug, pid, id, false).await
}

pub async fn progress(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Progress>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let m = load(&state, pid, id).await?;
    Ok(Json(
        state
            .store
            .module_progress(&m, c.row.project.estimate_type.0 == EstimateType::Points)
            .await?,
    ))
}

pub async fn links(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Vec<ModuleLink>>> {
    ctx(&state, &me, &slug, pid).await?;
    load(&state, pid, id).await?;
    Ok(Json(state.store.module_links(id).await?))
}

#[derive(Deserialize)]
pub struct LinkReq {
    url: String,
    #[serde(default)]
    title: String,
}

pub async fn add_link(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<LinkReq>,
) -> ApiResult<Json<Vec<ModuleLink>>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::EditCycleOrModule)?;
    live(load(&state, pid, id).await?)?;
    let url = req.url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://"))
        || url.len() > 2048
        || url.chars().any(char::is_whitespace)
    {
        return Err(Error::bad_request("links start with http:// or https://").into());
    }
    state
        .store
        .add_module_link(
            id,
            url,
            &req.title.trim().chars().take(200).collect::<String>(),
            me.id,
        )
        .await?;
    Ok(Json(state.store.module_links(id).await?))
}

pub async fn remove_link(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id, lid)): Path<(String, Uuid, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::EditCycleOrModule)?;
    live(load(&state, pid, id).await?)?;
    state.store.remove_module_link(id, lid).await?;
    Ok(StatusCode::NO_CONTENT)
}
