//! Cycles (SPRINT S2.1). Contributors create and edit; deleting is for the creator or a
//! project admin (PERMISSIONS §2.1).

use axum::extract::State;
use axum::http::StatusCode;
use np_core::cycle::CycleStatus;
use np_core::id::Uuid;
use np_core::perm::{Ownership, ProjAction};
use np_core::project::EstimateType;
use np_core::{Error, user, work_item};
use np_store::cycles::{Cycle, CyclePatch, Progress};
use serde::Deserialize;

use super::projects::{ProjCtx, load_project};
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};

async fn ctx(state: &AppState, me: &np_core::User, slug: &str, pid: Uuid) -> ApiResult<ProjCtx> {
    let c = load_project(state, slug, pid, me).await?;
    c.require(ProjAction::ViewWorkItems)?;
    if !c.row.project.feature_cycles {
        return Err(Error::forbidden("cycles are turned off for this project").into());
    }
    Ok(c)
}

async fn load(state: &AppState, pid: Uuid, cid: Uuid) -> ApiResult<Cycle> {
    Ok(state
        .store
        .cycle(pid, cid)
        .await?
        .ok_or_else(|| Error::not_found("cycle not found"))?)
}

/// Archived cycles are read-only until restored (as in Plane).
fn live(c: Cycle) -> ApiResult<Cycle> {
    if c.archived_at.is_some() {
        return Err(Error::conflict("this cycle is archived; restore it to make changes").into());
    }
    Ok(c)
}

fn clean_dates(
    s: Option<String>,
    e: Option<String>,
) -> ApiResult<(Option<String>, Option<String>)> {
    let d = |x: Option<String>| {
        x.filter(|v| !v.is_empty())
            .map(|v| work_item::validate_date(&v))
            .transpose()
    };
    let (s, e) = (d(s)?, d(e)?);
    if s.is_some() != e.is_some() {
        return Err(Error::bad_request("give both a start and an end date, or neither").into());
    }
    work_item::check_date_order(s.as_deref(), e.as_deref())?;
    Ok((s, e))
}

fn clean_name(n: &str) -> ApiResult<String> {
    let n = user::normalize_name(n, 80, "cycle name")?;
    if n.is_empty() {
        return Err(Error::bad_request("cycle name is required").into());
    }
    Ok(n)
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
) -> ApiResult<Json<Vec<Cycle>>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    if q.archived {
        c.require(ProjAction::ViewArchives)?;
    }
    Ok(Json(state.store.cycles(pid, q.archived).await?))
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    #[serde(default)]
    description: String,
    start_date: Option<String>,
    end_date: Option<String>,
    owner_id: Option<Uuid>,
}

pub async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Json(req): Json<CreateReq>,
) -> ApiResult<(StatusCode, Json<Cycle>)> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::CreateCycleOrModule)?;
    let (s, e) = clean_dates(req.start_date, req.end_date)?;
    let desc: String = req.description.trim().chars().take(4000).collect();
    let cy = state
        .store
        .create_cycle(
            pid,
            &clean_name(&req.name)?,
            &desc,
            s.as_deref(),
            e.as_deref(),
            req.owner_id.or(Some(me.id)),
            me.id,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(cy)))
}

pub async fn get(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, cid)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Cycle>> {
    ctx(&state, &me, &slug, pid).await?;
    Ok(Json(load(&state, pid, cid).await?))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    description: Option<String>,
    #[serde(default, with = "crate::util::double_option")]
    start_date: Option<Option<String>>,
    #[serde(default, with = "crate::util::double_option")]
    end_date: Option<Option<String>>,
    #[serde(default, with = "crate::util::double_option")]
    owner_id: Option<Option<Uuid>>,
}

pub async fn update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, cid)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<UpdateReq>,
) -> ApiResult<Json<Cycle>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::EditCycleOrModule)?;
    let cur = live(load(&state, pid, cid).await?)?;
    let dates = if req.start_date.is_some() || req.end_date.is_some() {
        Some(clean_dates(
            req.start_date.unwrap_or(cur.start_date.clone()),
            req.end_date.unwrap_or(cur.end_date.clone()),
        )?)
    } else {
        None
    };
    let p = CyclePatch {
        name: req.name.as_deref().map(clean_name).transpose()?,
        description: req
            .description
            .map(|d| d.trim().chars().take(4000).collect()),
        dates,
        owner_id: req.owner_id,
    };
    Ok(Json(state.store.update_cycle(pid, cid, &p).await?))
}

pub async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, cid)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let cy = live(load(&state, pid, cid).await?)?;
    let own = if cy.created_by == me.id {
        Ownership::Own
    } else {
        Ownership::Others
    };
    c.require_write(ProjAction::DeleteCycleOrModule(own))?;
    state.store.delete_cycle(cid).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_archived(
    state: AppState,
    me: np_core::User,
    slug: String,
    pid: Uuid,
    cid: Uuid,
    on: bool,
) -> ApiResult<Json<Cycle>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::EditCycleOrModule)?;
    let cy = load(&state, pid, cid).await?;
    if on && cy.archived_at.is_none() && cy.status != Some(CycleStatus::Completed) {
        return Err(Error::bad_request("only completed cycles can be archived").into());
    }
    state.store.set_cycle_archived(cid, on).await?;
    Ok(Json(load(&state, pid, cid).await?))
}

pub async fn archive(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, cid)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Cycle>> {
    set_archived(state, me, slug, pid, cid, true).await
}

pub async fn unarchive(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, cid)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Cycle>> {
    set_archived(state, me, slug, pid, cid, false).await
}

pub async fn progress(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, cid)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Progress>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let cy = load(&state, pid, cid).await?;
    let numeric = c.row.project.estimate_type.0 == EstimateType::Points;
    Ok(Json(state.store.cycle_progress(&cy, numeric).await?))
}

#[derive(Deserialize)]
pub struct TransferReq {
    to_cycle_id: Uuid,
}

pub async fn transfer(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, cid)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<TransferReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::EditWorkItem)?;
    load(&state, pid, cid).await?;
    load(&state, pid, req.to_cycle_id).await?;
    let moved = state
        .store
        .transfer_cycle(pid, cid, req.to_cycle_id, me.id)
        .await?;
    Ok(Json(serde_json::json!({ "moved": moved })))
}
