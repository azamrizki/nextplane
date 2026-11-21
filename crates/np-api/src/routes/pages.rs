//! Pages (SPRINT S3.1, PERMISSIONS §2.1): contributors create pages and edit their own;
//! admins edit any; commenters read; private pages are seen only by their owner; a locked
//! page is read-only until its owner or an admin unlocks it.

use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use np_core::id::Uuid;
use np_core::perm::{Ownership, ProjAction, ProjRole};
use np_core::{Error, User, user};
use np_store::pages::{Page, PageVersion};
use serde::{Deserialize, Serialize};

use super::projects::{ProjCtx, load_project};
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path, Query};
use crate::richtext;

pub(crate) async fn ctx(state: &AppState, me: &User, slug: &str, pid: Uuid) -> ApiResult<ProjCtx> {
    let c = load_project(state, slug, pid, me).await?;
    c.require(ProjAction::ViewWorkItems)?;
    if !c.row.project.feature_pages {
        return Err(Error::forbidden("pages are turned off for this project").into());
    }
    Ok(c)
}

/// A page the user may see (private ones only by their owner).
pub(crate) async fn load(state: &AppState, c: &ProjCtx, id: Uuid, me: &User) -> ApiResult<Page> {
    let p = state
        .store
        .page(c.id(), id)
        .await?
        .ok_or_else(|| Error::not_found("page not found"))?;
    if p.access == "private" && p.owner_id != me.id {
        return Err(Error::not_found("page not found").into());
    }
    Ok(p)
}

fn own(p: &Page, me: &User) -> Ownership {
    if p.owner_id == me.id {
        Ownership::Own
    } else {
        Ownership::Others
    }
}

/// Whether the user may change the page's content now.
pub fn can_edit(c: &ProjCtx, p: &Page, me: &User) -> bool {
    p.locked_at.is_none()
        && p.archived_at.is_none()
        && c.row.project.archived_at.is_none()
        && c.role
            .is_some_and(|r| np_core::perm::proj_can(r, ProjAction::EditPage(own(p, me))))
}

/// Owner or project admin: lock, archive, delete, change access.
fn can_manage(c: &ProjCtx, p: &Page, me: &User) -> bool {
    p.owner_id == me.id && c.role.is_some_and(|r| r >= ProjRole::Contributor)
        || c.role == Some(ProjRole::Admin)
}

#[derive(Serialize)]
pub struct PageOut {
    #[serde(flatten)]
    page: Page,
    can_edit: bool,
    can_manage: bool,
}

fn out(c: &ProjCtx, p: Page, me: &User) -> PageOut {
    PageOut {
        can_edit: can_edit(c, &p, me),
        can_manage: can_manage(c, &p, me),
        page: p,
    }
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
) -> ApiResult<Json<Vec<PageOut>>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let pages = state.store.pages(pid, me.id, q.archived).await?;
    Ok(Json(pages.into_iter().map(|p| out(&c, p, &me)).collect()))
}

#[derive(Deserialize)]
pub struct CreateReq {
    #[serde(default)]
    title: String,
    parent_id: Option<Uuid>,
    #[serde(default = "project_access")]
    access: String,
}

fn project_access() -> String {
    "project".into()
}

fn clean_title(t: &str) -> ApiResult<String> {
    Ok(user::normalize_name(t, 255, "page title")?)
}

fn clean_access(a: &str) -> ApiResult<&str> {
    match a {
        "project" | "private" => Ok(a),
        _ => Err(Error::bad_request("access is project or private").into()),
    }
}

pub async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Json(req): Json<CreateReq>,
) -> ApiResult<(StatusCode, Json<PageOut>)> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::CreatePage)?;
    let p = state
        .store
        .create_page(
            c.ws.workspace.id,
            pid,
            me.id,
            &clean_title(&req.title)?,
            req.parent_id,
            clean_access(&req.access)?,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(out(&c, p, &me))))
}

pub async fn get(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<PageOut>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let p = load(&state, &c, id, &me).await?;
    Ok(Json(out(&c, p, &me)))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    title: Option<String>,
    /// Initial text of a page nobody has opened in the editor yet (afterwards the text
    /// changes only through the live document).
    html: Option<String>,
    access: Option<String>,
    #[serde(default, with = "crate::util::double_option")]
    parent_id: Option<Option<Uuid>>,
    sort_order: Option<f64>,
}

pub async fn update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<UpdateReq>,
) -> ApiResult<Json<PageOut>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let p = load(&state, &c, id, &me).await?;
    let content = req.title.is_some() || req.html.is_some();
    let moving = req.parent_id.is_some() || req.sort_order.is_some();
    if (content || moving) && !can_edit(&c, &p, &me) {
        return Err(Error::forbidden(if p.locked_at.is_some() {
            "this page is locked"
        } else {
            "you cannot edit this page"
        })
        .into());
    }
    if req.access.is_some() && p.owner_id != me.id {
        return Err(Error::forbidden("only the owner changes who can see a page").into());
    }
    if req.html.is_some() && state.store.page_ydoc(id).await?.is_some() {
        return Err(
            Error::conflict("this page is edited live; change its text in the editor").into(),
        );
    }
    if req.sort_order.is_some_and(|o| !o.is_finite()) {
        return Err(Error::bad_request("bad sort order").into());
    }
    let title = req.title.as_deref().map(clean_title).transpose()?;
    let html = req
        .html
        .as_deref()
        .map(|h| richtext::clean_body(None, h).map(|x| x.1))
        .transpose()?;
    let access = req.access.as_deref().map(clean_access).transpose()?;
    let p = state
        .store
        .update_page(
            pid,
            id,
            me.id,
            title.as_deref(),
            html.as_deref(),
            access,
            req.parent_id,
            req.sort_order,
        )
        .await?;
    if access.is_some() {
        state.rooms.kick(id);
    }
    Ok(Json(out(&c, p, &me)))
}

#[derive(Deserialize)]
pub struct FlagPath {
    slug: String,
    /// The project.
    id: Uuid,
    page: Uuid,
    action: String,
}

/// `lock`, `unlock`, `archive`, `unarchive`.
pub async fn flag(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(fp): Path<FlagPath>,
) -> ApiResult<Json<PageOut>> {
    let c = ctx(&state, &me, &fp.slug, fp.id).await?;
    let p = load(&state, &c, fp.page, &me).await?;
    if !can_manage(&c, &p, &me) || c.row.project.archived_at.is_some() {
        return Err(
            Error::forbidden("only the page's owner or a project admin can do this").into(),
        );
    }
    let (flag, on) = match fp.action.as_str() {
        "lock" => ("locked", true),
        "unlock" => ("locked", false),
        "archive" => ("archived", true),
        "unarchive" => ("archived", false),
        _ => return Err(Error::bad_request("unknown action").into()),
    };
    state.store.set_page_flag(fp.page, flag, on, me.id).await?;
    state.rooms.kick(fp.page);
    Ok(Json(out(
        &c,
        state
            .store
            .page(fp.id, fp.page)
            .await?
            .ok_or_else(|| Error::not_found("page not found"))?,
        &me,
    )))
}

/// `POST /pages/{pid}/duplicate`: a copy you own, next to the original.
pub async fn duplicate(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<(StatusCode, Json<PageOut>)> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::CreatePage)?;
    let p = load(&state, &c, id, &me).await?;
    let base = if p.title.is_empty() {
        "Untitled".to_string()
    } else {
        p.title.clone()
    };
    let title: String = format!("{base} (copy)").chars().take(255).collect();
    let copy = state.store.duplicate_page(pid, id, me.id, &title).await?;
    Ok((StatusCode::CREATED, Json(out(&c, copy, &me))))
}

pub async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let p = load(&state, &c, id, &me).await?;
    if !can_manage(&c, &p, &me) || c.row.project.archived_at.is_some() {
        return Err(
            Error::forbidden("only the page's owner or a project admin can delete it").into(),
        );
    }
    state
        .store
        .set_page_flag(id, "deleted", true, me.id)
        .await?;
    state.rooms.kick(id);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn versions(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Vec<PageVersion>>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    load(&state, &c, id, &me).await?;
    Ok(Json(state.store.page_versions(id).await?))
}

/// Keeps a version of the page as it is now (before a restore, or on request).
pub async fn save_version(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let p = load(&state, &c, id, &me).await?;
    if !can_edit(&c, &p, &me) {
        return Err(Error::forbidden("you cannot edit this page").into());
    }
    state.store.add_page_version(id, Some(me.id)).await?;
    Ok(StatusCode::CREATED)
}

pub async fn version(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id, vid)): Path<(String, Uuid, Uuid, Uuid)>,
) -> ApiResult<Json<PageVersion>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    load(&state, &c, id, &me).await?;
    Ok(Json(
        state
            .store
            .page_version(id, vid)
            .await?
            .ok_or_else(|| Error::not_found("version not found"))?,
    ))
}

/// `GET /api/v1/collab/{page_id}`: the y-websocket endpoint of a page.
pub async fn collab(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    up: WebSocketUpgrade,
) -> Response {
    if !crate::origin::websocket_allowed(&headers) {
        return ApiError(Error::forbidden("cross-origin socket refused")).into_response();
    }
    let result: ApiResult<(bool, Uuid)> = async {
        let (ws_id, pid) = state
            .store
            .page_home(id)
            .await?
            .ok_or_else(|| Error::not_found("page not found"))?;
        let slug = state
            .store
            .workspace_slug(ws_id)
            .await?
            .ok_or_else(|| Error::not_found("page not found"))?;
        let c = ctx(&state, &me, &slug, pid).await?;
        let p = load(&state, &c, id, &me).await?;
        Ok((can_edit(&c, &p, &me), pid))
    }
    .await;
    let (can_write, pid) = match result {
        Ok(w) => w,
        Err(e) => return e.into_response(),
    };
    let rooms = state.rooms.clone();
    let store = state.store.clone();
    let room = match rooms.open(&store, id, pid).await {
        Ok(r) => r,
        Err(e) => return ApiError::from(e).into_response(),
    };
    up.max_message_size(16 * 1024 * 1024)
        .on_upgrade(move |socket| crate::pagesync::serve(store, room, socket, me.id, can_write))
}
