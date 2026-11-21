//! A member's profile page (S3.7, as in Plane): who they are, their numbers and their
//! activity, counted only in the projects the *viewer* belongs to and may read.

use std::collections::HashMap;

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use np_core::id::Uuid;
use np_core::perm::WsRole;
use np_core::{Error, User};
use np_store::profile::{ActivityEntry, Count, ProjectNumbers};
use serde::{Deserialize, Serialize};

use super::views::readable_projects;
use super::workspaces::load;
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};

#[derive(Serialize)]
pub struct Person {
    id: Uuid,
    name: String,
    /// Left out for guests looking at someone else (as in Plane).
    email: Option<String>,
    avatar: Option<String>,
    role: WsRole,
    joined_at: String,
}

#[derive(Serialize)]
pub struct ProjectRow {
    id: Uuid,
    name: String,
    identifier: String,
    icon: Option<String>,
    cover_color: Option<String>,
    #[serde(flatten)]
    numbers: ProjectNumbers,
}

#[derive(Serialize)]
pub struct Profile {
    user: Person,
    created: i64,
    assigned: i64,
    subscribed: i64,
    pending: i64,
    completed: i64,
    /// Assigned items by state group and by priority.
    by_state_group: Vec<Count>,
    by_priority: Vec<Count>,
    /// Per project; only for workspace members and admins looking.
    projects: Option<Vec<ProjectRow>>,
}

async fn person(state: &AppState, ws: Uuid, uid: Uuid) -> ApiResult<np_store::workspaces::WsMember> {
    state
        .store
        .workspace_members(ws)
        .await?
        .into_iter()
        .find(|m| m.user_id == uid)
        .ok_or_else(|| Error::not_found("this person is not in the workspace").into())
}

/// `GET /workspaces/{slug}/members/{uid}/profile`: any workspace member may look.
pub async fn profile(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, uid)): Path<(String, Uuid)>,
) -> ApiResult<Json<Profile>> {
    let ws = load(&state, &slug, &me).await?;
    let m = person(&state, ws.workspace.id, uid).await?;
    let projects = readable_projects(&state, &ws, &me).await?;
    let ids: Vec<Uuid> = projects.iter().map(|p| p.id).collect();
    let numbers = state.store.profile_numbers(&ids, uid).await?;
    let sum = |f: fn(&ProjectNumbers) -> i64| numbers.iter().map(f).sum::<i64>();
    let by_id: HashMap<Uuid, &ProjectNumbers> = numbers.iter().map(|n| (n.project_id, n)).collect();
    let guest = ws.role == WsRole::Guest;
    Ok(Json(Profile {
        created: sum(|n| n.created),
        assigned: sum(|n| n.assigned),
        pending: sum(|n| n.pending),
        completed: sum(|n| n.completed),
        subscribed: state.store.profile_subscribed(&ids, uid).await?,
        by_state_group: state.store.profile_assigned_by(&ids, uid, false).await?,
        by_priority: state.store.profile_assigned_by(&ids, uid, true).await?,
        projects: (!guest).then(|| {
            projects
                .iter()
                .map(|p| ProjectRow {
                    id: p.id,
                    name: p.name.clone(),
                    identifier: p.identifier.clone(),
                    icon: p.icon.clone(),
                    cover_color: p.cover_color.clone(),
                    numbers: by_id
                        .get(&p.id)
                        .map(|n| (*n).clone())
                        .unwrap_or(ProjectNumbers {
                            project_id: p.id,
                            ..Default::default()
                        }),
                })
                .collect()
        }),
        user: Person {
            id: m.user_id,
            name: m.name,
            email: (!guest || uid == me.id).then_some(m.email),
            avatar: m.avatar,
            role: m.role,
            joined_at: m.joined_at,
        },
    }))
}

#[derive(Deserialize)]
pub struct ActivityQuery {
    cursor: Option<String>,
    limit: Option<i64>,
    /// One UTC day, `YYYY-MM-DD`.
    date: Option<String>,
    /// `csv` downloads the day's activity.
    format: Option<String>,
}

#[derive(Serialize)]
pub struct ActivityOut {
    #[serde(flatten)]
    entry: ActivityEntry,
    identifier: String,
    project_name: String,
}

fn cursor_of(e: &ActivityEntry) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{}|{}", e.created_at, e.id.simple()))
}

fn parse_cursor(c: &str) -> ApiResult<(String, Uuid)> {
    use base64::Engine;
    let bad = || Error::bad_request("bad cursor");
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(c)
        .map_err(|_| bad())?;
    let s = String::from_utf8(raw).map_err(|_| bad())?;
    let (at, id) = s.split_once('|').ok_or_else(bad)?;
    Ok((at.to_string(), id.parse().map_err(|_| bad())?))
}

/// `GET /workspaces/{slug}/members/{uid}/activity`: workspace members and admins only. Pages
/// of `limit` (default 100) with `X-Next-Cursor`; `date` limits to one day; `format=csv`
/// downloads it (at most 10,000 rows).
pub async fn activity(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, uid)): Path<(String, Uuid)>,
    Query(q): Query<ActivityQuery>,
) -> ApiResult<Response> {
    let ws = load(&state, &slug, &me).await?;
    if ws.role == WsRole::Guest {
        return Err(Error::forbidden("guests see only the summary of a profile").into());
    }
    let m = person(&state, ws.workspace.id, uid).await?;
    let date = q
        .date
        .as_deref()
        .map(np_core::work_item::validate_date)
        .transpose()?;
    let csv = q.format.as_deref() == Some("csv");
    if csv && date.is_none() {
        return Err(Error::bad_request("a download is for one day: add date=YYYY-MM-DD").into());
    }
    let projects = readable_projects(&state, &ws, &me).await?;
    let names: HashMap<Uuid, (String, String)> = projects
        .iter()
        .map(|p| (p.id, (p.identifier.clone(), p.name.clone())))
        .collect();
    let ids: Vec<Uuid> = projects.iter().map(|p| p.id).collect();
    let limit = if csv { 10_000 } else { q.limit.unwrap_or(100).clamp(1, 500) };
    let rows = state
        .store
        .user_activity(
            &ids,
            uid,
            q.cursor.as_deref().map(parse_cursor).transpose()?,
            date.as_deref(),
            limit,
        )
        .await?;
    if csv {
        return Ok(csv_response(&me, &m, &names, &rows, date.as_deref().unwrap_or("")));
    }
    let next = (rows.len() as i64 == limit).then(|| rows.last().map(cursor_of)).flatten();
    let out: Vec<ActivityOut> = rows
        .into_iter()
        .map(|entry| {
            let (identifier, project_name) = names.get(&entry.project_id).cloned().unwrap_or_default();
            ActivityOut {
                entry,
                identifier,
                project_name,
            }
        })
        .collect();
    let mut res = Json(out).into_response();
    if let Some(c) = next.and_then(|c| c.parse().ok()) {
        res.headers_mut().insert("x-next-cursor", c);
    }
    Ok(res)
}

fn csv_response(
    _me: &User,
    m: &np_store::workspaces::WsMember,
    names: &HashMap<Uuid, (String, String)>,
    rows: &[ActivityEntry],
    day: &str,
) -> Response {
    let mut w = csv::Writer::from_writer(vec![0xEF, 0xBB, 0xBF]);
    let _ = w.write_record(["Actor", "Work item", "Project", "When", "Action", "Field", "Old value", "New value"]);
    // Cells that a spreadsheet would read as a formula are prefixed (as the project export does).
    let safe = |s: &str| {
        if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
            format!("'{s}")
        } else {
            s.to_string()
        }
    };
    let who = if m.name.is_empty() { m.email.clone() } else { m.name.clone() };
    for r in rows {
        let (key, project) = names.get(&r.project_id).cloned().unwrap_or_default();
        let _ = w.write_record([
            safe(&who),
            format!("{key}-{}", r.sequence),
            safe(&project),
            r.created_at.clone(),
            r.verb.clone(),
            r.field.clone().unwrap_or_default(),
            safe(r.old_value.as_deref().unwrap_or("")),
            safe(r.new_value.as_deref().unwrap_or("")),
        ]);
    }
    let body = w.into_inner().unwrap_or_default();
    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"activity-{day}.csv\""),
            ),
        ],
        body,
    )
        .into_response()
}
