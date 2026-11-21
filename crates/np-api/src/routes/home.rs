//! The home page's widgets (S3.7, as in Plane): widget preferences, quick links, recent
//! visits and stickies. All of it is personal: one person, one workspace.

use axum::extract::State;
use axum::http::StatusCode;
use np_core::Error;
use np_core::id::Uuid;
use np_core::perm::ProjAction;
use np_store::home::{QuickLink, Sticky, StickyPatch, WIDGETS, Widget};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::projects::load_project;
use super::workspaces::load;
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};
use crate::richtext;

pub async fn widgets(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<Widget>>> {
    let ws = load(&state, &slug, &me).await?;
    Ok(Json(state.store.home_widgets(ws.workspace.id, me.id).await?))
}

#[derive(Deserialize)]
pub struct WidgetReq {
    is_enabled: Option<bool>,
    sort_order: Option<f64>,
}

pub async fn set_widget(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, key)): Path<(String, String)>,
    Json(req): Json<WidgetReq>,
) -> ApiResult<Json<Vec<Widget>>> {
    let ws = load(&state, &slug, &me).await?;
    if !WIDGETS.contains(&key.as_str()) {
        return Err(Error::not_found("no such widget").into());
    }
    if req.sort_order.is_some_and(|o| !o.is_finite()) {
        return Err(Error::bad_request("sort_order must be a number").into());
    }
    state
        .store
        .set_home_widget(ws.workspace.id, me.id, &key, req.is_enabled, req.sort_order)
        .await?;
    Ok(Json(state.store.home_widgets(ws.workspace.id, me.id).await?))
}

// ---- Quick links ---------------------------------------------------------------------------

/// `example.com/x` → `http://example.com/x` (as in Plane); only http(s) addresses.
fn clean_url(raw: &str) -> ApiResult<String> {
    let u = raw.trim();
    let u = if u.contains("://") {
        u.to_string()
    } else {
        format!("http://{u}")
    };
    let ok = (u.starts_with("http://") || u.starts_with("https://"))
        && u.len() <= 2048
        && !u.chars().any(char::is_whitespace)
        && reqwest::Url::parse(&u).is_ok_and(|p| p.host_str().is_some_and(|h| !h.is_empty()));
    if !ok {
        return Err(Error::bad_request("enter a web address, like https://example.com").into());
    }
    Ok(u)
}

fn clean_title(t: &str) -> String {
    t.trim().chars().take(255).collect()
}

pub async fn links(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<QuickLink>>> {
    let ws = load(&state, &slug, &me).await?;
    Ok(Json(state.store.quick_links(ws.workspace.id, me.id).await?))
}

#[derive(Deserialize)]
pub struct LinkReq {
    url: Option<String>,
    title: Option<String>,
}

pub async fn add_link(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<LinkReq>,
) -> ApiResult<(StatusCode, Json<QuickLink>)> {
    let ws = load(&state, &slug, &me).await?;
    let url = clean_url(req.url.as_deref().unwrap_or(""))?;
    let title = clean_title(req.title.as_deref().unwrap_or(""));
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .add_quick_link(ws.workspace.id, me.id, &url, &title)
                .await?,
        ),
    ))
}

pub async fn update_link(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
    Json(req): Json<LinkReq>,
) -> ApiResult<Json<QuickLink>> {
    let ws = load(&state, &slug, &me).await?;
    let url = req.url.as_deref().map(clean_url).transpose()?;
    let title = req.title.as_deref().map(clean_title);
    Ok(Json(
        state
            .store
            .update_quick_link(ws.workspace.id, me.id, id, url.as_deref(), title.as_deref())
            .await?,
    ))
}

pub async fn delete_link(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    let ws = load(&state, &slug, &me).await?;
    state
        .store
        .delete_quick_link(ws.workspace.id, me.id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Recents --------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct VisitReq {
    entity: String,
    id: Uuid,
    /// The project of a work item or page.
    project_id: Option<Uuid>,
}

/// Records that you opened something (the web app calls this). Only things you may see.
pub async fn visit(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<VisitReq>,
) -> ApiResult<StatusCode> {
    let ws = load(&state, &slug, &me).await?;
    let project = match req.entity.as_str() {
        "project" => {
            let c = load_project(&state, &slug, req.id, &me).await?;
            c.require(ProjAction::ViewWorkItems)?;
            None
        }
        "work_item" | "page" => {
            let pid = req
                .project_id
                .ok_or_else(|| Error::bad_request("project_id is required"))?;
            resolve(&state, &slug, &me, &req.entity, req.id, pid)
                .await
                .ok_or_else(|| Error::not_found("not found"))?;
            Some(pid)
        }
        _ => return Err(Error::bad_request("entity is work_item, page or project").into()),
    };
    state
        .store
        .record_visit(ws.workspace.id, me.id, project, &req.entity, req.id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// What the recents widget shows of a work item or page, if you may still see it.
async fn resolve(
    state: &AppState,
    slug: &str,
    me: &np_core::User,
    entity: &str,
    id: Uuid,
    pid: Uuid,
) -> Option<Value> {
    match entity {
        "work_item" => {
            let c = load_project(state, slug, pid, me).await.ok()?;
            c.require(ProjAction::ViewWorkItems).ok()?;
            let w = state.store.work_item(pid, id).await.ok()??;
            if w.is_draft && w.created_by != me.id {
                return None;
            }
            let st = state
                .store
                .states(pid)
                .await
                .ok()?
                .into_iter()
                .find(|s| s.id == w.state_id);
            Some(json!({
                "id": w.id, "project_id": pid, "identifier": c.row.project.identifier, "sequence": w.sequence,
                "title": w.title, "priority": w.priority, "assignee_ids": w.assignee_ids,
                "state": st.map(|s| json!({ "name": s.name, "color": s.color, "group": s.group })),
                "archived": w.archived_at.is_some(),
            }))
        }
        "page" => {
            let c = super::pages::ctx(state, me, slug, pid).await.ok()?;
            let p = super::pages::load(state, &c, id, me).await.ok()?;
            Some(json!({ "id": p.id, "project_id": pid, "title": p.title, "owner_id": p.owner_id, "project_name": c.row.project.name }))
        }
        _ => None,
    }
}

#[derive(Deserialize)]
pub struct RecentsQuery {
    entity: Option<String>,
}

#[derive(Serialize)]
pub struct Recent {
    entity: String,
    visited_at: String,
    #[serde(flatten)]
    data: Value,
}

/// Your recent visits, most recent first, with what each shows; things you can no longer see
/// are left out.
pub async fn recents(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Query(q): Query<RecentsQuery>,
) -> ApiResult<Json<Vec<Recent>>> {
    let ws = load(&state, &slug, &me).await?;
    let mut out = Vec::new();
    for v in state.store.recent_visits(ws.workspace.id, me.id).await? {
        if q.entity.as_deref().is_some_and(|e| e != v.entity) {
            continue;
        }
        let data = match (v.entity.as_str(), v.project_id) {
            ("project", _) => match load_project(&state, &slug, v.entity_id, &me).await {
                Ok(c) if c.require(ProjAction::ViewWorkItems).is_ok() => Some(json!({
                    "id": c.row.project.id, "name": c.row.project.name, "identifier": c.row.project.identifier,
                    "icon": c.row.project.icon, "cover_color": c.row.project.cover_color,
                })),
                _ => None,
            },
            (e, Some(pid)) => resolve(&state, &slug, &me, e, v.entity_id, pid).await,
            _ => None,
        };
        if let Some(data) = data {
            out.push(Recent {
                entity: v.entity,
                visited_at: v.visited_at,
                data,
            });
        }
    }
    Ok(Json(out))
}

// ---- Stickies -------------------------------------------------------------------------------

/// The eight sticky colours (as in Plane).
pub const COLORS: [&str; 8] = [
    "gray",
    "peach",
    "pink",
    "orange",
    "green",
    "light-blue",
    "dark-blue",
    "purple",
];

#[derive(Deserialize)]
pub struct StickiesQuery {
    q: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn stickies(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Query(q): Query<StickiesQuery>,
) -> ApiResult<Json<Vec<Sticky>>> {
    let ws = load(&state, &slug, &me).await?;
    Ok(Json(
        state
            .store
            .stickies(
                ws.workspace.id,
                me.id,
                q.q.as_deref(),
                q.limit.unwrap_or(30).clamp(1, 100),
                q.offset.unwrap_or(0).max(0),
            )
            .await?,
    ))
}

#[derive(Deserialize)]
pub struct StickyReq {
    description_json: Option<Value>,
    description_html: Option<String>,
    color: Option<String>,
    sort_order: Option<f64>,
}

fn body(req: &StickyReq) -> ApiResult<Option<(Option<String>, String, String)>> {
    let Some(html) = req.description_html.as_deref() else {
        return Ok(None);
    };
    if html.len() > 64 * 1024 {
        return Err(Error::bad_request("a sticky holds up to 64 KB").into());
    }
    let (json, html) = richtext::clean_body(req.description_json.clone(), html)?;
    let text = richtext::plain_text(&html);
    Ok(Some((json, html, text)))
}

fn color(c: Option<&str>) -> ApiResult<Option<String>> {
    match c {
        None => Ok(None),
        Some(c) if COLORS.contains(&c) => Ok(Some(c.to_string())),
        Some(_) => Err(Error::bad_request(format!("colour is one of {}", COLORS.join(", "))).into()),
    }
}

pub async fn create_sticky(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<StickyReq>,
) -> ApiResult<(StatusCode, Json<Sticky>)> {
    let ws = load(&state, &slug, &me).await?;
    let b = body(&req)?.unwrap_or((None, String::new(), String::new()));
    // A new sticky gets one of the colours at random, unless one is asked for.
    let c = color(req.color.as_deref())?.unwrap_or_else(|| {
        let mut n = [0u8; 1];
        let _ = getrandom::fill(&mut n);
        COLORS[n[0] as usize % COLORS.len()].to_string()
    });
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_sticky(ws.workspace.id, me.id, b, &c).await?),
    ))
}

pub async fn update_sticky(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
    Json(req): Json<StickyReq>,
) -> ApiResult<Json<Sticky>> {
    let ws = load(&state, &slug, &me).await?;
    if req.sort_order.is_some_and(|o| !o.is_finite()) {
        return Err(Error::bad_request("sort_order must be a number").into());
    }
    let p = StickyPatch {
        body: body(&req)?,
        color: color(req.color.as_deref())?,
        sort_order: req.sort_order,
    };
    Ok(Json(
        state
            .store
            .update_sticky(ws.workspace.id, me.id, id, &p)
            .await?,
    ))
}

pub async fn delete_sticky(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    let ws = load(&state, &slug, &me).await?;
    state.store.delete_sticky(ws.workspace.id, me.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
