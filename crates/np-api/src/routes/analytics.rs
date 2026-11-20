//! Analytics (SPRINT S3.4): only projects the caller may read count. Workspace guests get
//! no analytics (they see only their own intake).

use axum::extract::State;
use np_core::id::Uuid;
use np_core::perm::{self, ProjAction, WsRole};
use np_core::{Error, User};
use np_store::analytics::{Breakdown, Dim, Measure, Overview};
use serde::{Deserialize, Serialize};

use super::workspaces::load;
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};

/// Readable projects of the workspace, or the one asked for.
async fn scope(
    state: &AppState,
    slug: &str,
    me: &User,
    project: Option<Uuid>,
) -> ApiResult<Vec<Uuid>> {
    let ws = load(state, slug, me).await?;
    if ws.role == WsRole::Guest {
        return Err(Error::forbidden("guests have no analytics").into());
    }
    let rows = state
        .store
        .workspace_projects(ws.workspace.id, me.id, false)
        .await?;
    let readable: Vec<Uuid> = rows
        .iter()
        .filter(|r| perm::can_see_project(ws.role, r.member_role.0, r.project.is_private))
        .filter(|r| {
            perm::effective_proj_role(ws.role, r.member_role.0)
                .is_some_and(|role| perm::proj_can(role, ProjAction::ViewWorkItems))
        })
        .map(|r| r.project.id)
        .collect();
    match project {
        Some(p) if readable.contains(&p) => Ok(vec![p]),
        Some(_) => Err(Error::not_found("project not found").into()),
        None => Ok(readable),
    }
}

#[derive(Deserialize)]
pub struct OverviewQuery {
    project: Option<Uuid>,
}

#[derive(Serialize)]
pub struct OverviewOut {
    #[serde(flatten)]
    numbers: Overview,
    by_state_group: Breakdown,
    by_priority: Breakdown,
    by_assignee: Breakdown,
}

pub async fn overview(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Query(q): Query<OverviewQuery>,
) -> ApiResult<Json<OverviewOut>> {
    let ps = scope(&state, &slug, &me, q.project).await?;
    let s = &state.store;
    Ok(Json(OverviewOut {
        numbers: s.overview(&ps, 12, time::OffsetDateTime::now_utc()).await?,
        by_state_group: s
            .breakdown(&ps, Dim::StateGroup, None, Measure::Count)
            .await?,
        by_priority: s
            .breakdown(&ps, Dim::Priority, None, Measure::Count)
            .await?,
        by_assignee: s
            .breakdown(&ps, Dim::Assignee, None, Measure::Count)
            .await?,
    }))
}

#[derive(Deserialize)]
pub struct BreakdownQuery {
    project: Option<Uuid>,
    x: String,
    segment: Option<String>,
    #[serde(default)]
    measure: Option<String>,
}

pub async fn breakdown(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Query(q): Query<BreakdownQuery>,
) -> ApiResult<Json<Breakdown>> {
    let dim = |s: &str| {
        Dim::parse(s).ok_or_else(|| {
            Error::bad_request(
                "dimensions: state_group, state, priority, assignee, label, cycle, module, project",
            )
        })
    };
    let x = dim(&q.x)?;
    let segment = q
        .segment
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(dim)
        .transpose()?;
    let measure = match q.measure.as_deref().unwrap_or("count") {
        "count" => Measure::Count,
        "points" => Measure::Points,
        _ => return Err(Error::bad_request("measure is count or points").into()),
    };
    let ps = scope(&state, &slug, &me, q.project).await?;
    Ok(Json(state.store.breakdown(&ps, x, segment, measure).await?))
}
