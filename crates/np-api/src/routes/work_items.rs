//! Work items (PERMISSIONS §2.1).

use crate::extract::Json;
use crate::extract::{Path, Query};
use axum::extract::State;
use axum::http::StatusCode;
use np_core::Error;
use np_core::id::Uuid;
use np_core::perm::{Ownership, ProjAction};
use np_core::work_item::{self, Priority, RelationKind};
use np_store::work_items::{Activity, Filter, NewWorkItem, Relation, WorkItem, WorkItemPatch};
use serde::Deserialize;

use super::projects::{ProjCtx, load_project};
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::richtext;

impl ProjCtx {
    /// Writes are refused in archived projects (read-only).
    pub fn require_write(&self, action: ProjAction) -> ApiResult<()> {
        self.require(action)?;
        if self.row.project.archived_at.is_some() {
            return Err(
                Error::conflict("this project is archived; restore it to make changes").into(),
            );
        }
        Ok(())
    }
}

fn ids(raw: &Option<String>) -> ApiResult<Vec<Uuid>> {
    raw.as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<Uuid>()
                .map_err(|_| Error::bad_request(format!("bad id {s:?}")).into())
        })
        .collect()
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    state: Option<String>,
    state_group: Option<String>,
    priority: Option<String>,
    assignee: Option<String>,
    #[serde(default)]
    unassigned: bool,
    label: Option<String>,
    created_by: Option<String>,
    parent: Option<Uuid>,
    /// Cycle ids, comma-separated; `none` for items without a cycle.
    cycle: Option<String>,
    /// Module ids, comma-separated; `none` for items in no module.
    module: Option<String>,
    #[serde(default)]
    archived: bool,
    due_before: Option<String>,
    due_after: Option<String>,
    q: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
}

pub const PAGE_DEFAULT: i64 = 1000;
pub const PAGE_MAX: i64 = 5000;

fn encode_cursor(o: f64, seq: i64) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{o}:{seq}"))
}

fn decode_cursor(c: &str) -> ApiResult<(f64, i64)> {
    use base64::Engine;
    let bad = || Error::bad_request("bad cursor");
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(c)
        .map_err(|_| bad())?;
    let s = String::from_utf8(raw).map_err(|_| bad())?;
    let (o, q) = s.split_once(':').ok_or_else(bad)?;
    let o: f64 = o.parse().map_err(|_| bad())?;
    if !o.is_finite() {
        return Err(bad().into());
    }
    Ok((o, q.parse().map_err(|_| bad())?))
}

/// Pages of at most `limit` (default 1000) items in board order; the next page's cursor is in
/// the `X-Next-Cursor` header (absent on the last page).
pub async fn list(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Query(q): Query<ListQuery>,
) -> ApiResult<impl axum::response::IntoResponse> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    ctx.require(ProjAction::ViewWorkItems)?;
    let f = Filter {
        state_ids: ids(&q.state)?,
        state_groups: q
            .state_group
            .as_deref()
            .unwrap_or("")
            .split(',')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        priorities: q
            .priority
            .as_deref()
            .unwrap_or("")
            .split(',')
            .filter(|s| !s.is_empty())
            .map(Priority::parse)
            .collect::<Result<_, _>>()?,
        assignee_ids: ids(&q.assignee)?,
        unassigned: q.unassigned,
        label_ids: ids(&q.label)?,
        created_by: ids(&q.created_by)?,
        parent_id: q.parent,
        cycle_ids: ids(&q.cycle.as_deref().map(|c| {
            c.split(',')
                .filter(|x| *x != "none")
                .collect::<Vec<_>>()
                .join(",")
        }))?,
        no_cycle: q
            .cycle
            .as_deref()
            .is_some_and(|c| c.split(',').any(|x| x == "none")),
        module_ids: ids(&q.module.as_deref().map(|c| {
            c.split(',')
                .filter(|x| *x != "none")
                .collect::<Vec<_>>()
                .join(",")
        }))?,
        no_module: q
            .module
            .as_deref()
            .is_some_and(|c| c.split(',').any(|x| x == "none")),
        archived: q.archived,
        due_before: q
            .due_before
            .as_deref()
            .map(work_item::validate_date)
            .transpose()?,
        due_after: q
            .due_after
            .as_deref()
            .map(work_item::validate_date)
            .transpose()?,
        search: q.q,
        only_created_by: None,
        subscribed_by: None,
        drafts_of: None,
        limit: Some(q.limit.unwrap_or(PAGE_DEFAULT).clamp(1, PAGE_MAX)),
        after: q.cursor.as_deref().map(decode_cursor).transpose()?,
    };
    let limit = f.limit.unwrap_or(PAGE_DEFAULT);
    let items = state.store.work_items(pid, &f).await?;
    let mut headers = axum::http::HeaderMap::new();
    if items.len() as i64 == limit
        && let Some(last) = items.last()
        && let Ok(v) = encode_cursor(last.sort_order, last.sequence).parse()
    {
        headers.insert("x-next-cursor", v);
    }
    Ok((headers, Json(items)))
}

#[derive(Deserialize)]
pub struct CreateReq {
    title: String,
    description_json: Option<serde_json::Value>,
    #[serde(default)]
    description_html: String,
    state_id: Option<Uuid>,
    priority: Option<Priority>,
    start_date: Option<String>,
    due_date: Option<String>,
    estimate: Option<String>,
    parent_id: Option<Uuid>,
    cycle_id: Option<Uuid>,
    assignee_ids: Option<Vec<Uuid>>,
    #[serde(default)]
    label_ids: Vec<Uuid>,
    #[serde(default)]
    module_ids: Vec<Uuid>,
    #[serde(default)]
    is_draft: bool,
}

fn opt_date(d: Option<String>) -> ApiResult<Option<String>> {
    Ok(d.filter(|d| !d.is_empty())
        .map(|d| work_item::validate_date(&d))
        .transpose()?)
}

pub async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Json(req): Json<CreateReq>,
) -> ApiResult<(StatusCode, Json<WorkItem>)> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    ctx.require_write(ProjAction::CreateWorkItem)?;
    let (json, html) = richtext::clean_body(req.description_json, &req.description_html)?;
    let start_date = opt_date(req.start_date)?;
    let due_date = opt_date(req.due_date)?;
    work_item::check_date_order(start_date.as_deref(), due_date.as_deref())?;
    let item = state
        .store
        .create_work_item(
            pid,
            &NewWorkItem {
                title: work_item::clean_title(&req.title)?,
                description_json: json,
                description_html: html,
                state_id: req.state_id,
                priority: req.priority,
                start_date,
                due_date,
                estimate: req.estimate.filter(|e| !e.is_empty()),
                parent_id: req.parent_id,
                cycle_id: req.cycle_id,
                assignee_ids: req.assignee_ids,
                label_ids: req.label_ids,
                module_ids: dedup(req.module_ids),
                is_draft: req.is_draft,
                in_intake: false,
            },
            me.id,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn load_item(state: &AppState, ctx: &ProjCtx, id: Uuid) -> ApiResult<WorkItem> {
    ctx.require(ProjAction::ViewWorkItems)?;
    let item = state
        .store
        .work_item(ctx.id(), id)
        .await?
        .ok_or_else(|| Error::not_found("work item not found"))?;
    // Drafts are private to their author.
    if item.is_draft && Some(item.created_by) != ctx.viewer {
        return Err(Error::not_found("work item not found").into());
    }
    Ok(item)
}

/// Intake submissions change only through triage (admins); submitters use the intake API.
fn require_triager(ctx: &ProjCtx, item: &WorkItem) -> ApiResult<()> {
    if item.in_intake && ctx.role != Some(np_core::perm::ProjRole::Admin) {
        return Err(Error::forbidden("this work item is still in intake").into());
    }
    Ok(())
}

pub async fn get(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<WorkItem>> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    Ok(Json(load_item(&state, &ctx, id).await?))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    title: Option<String>,
    #[serde(default, with = "crate::util::double_option")]
    description_json: Option<Option<serde_json::Value>>,
    description_html: Option<String>,
    state_id: Option<Uuid>,
    priority: Option<Priority>,
    #[serde(default, with = "crate::util::double_option")]
    start_date: Option<Option<String>>,
    #[serde(default, with = "crate::util::double_option")]
    due_date: Option<Option<String>>,
    #[serde(default, with = "crate::util::double_option")]
    estimate: Option<Option<String>>,
    #[serde(default, with = "crate::util::double_option")]
    parent_id: Option<Option<Uuid>>,
    #[serde(default, with = "crate::util::double_option")]
    cycle_id: Option<Option<Uuid>>,
    sort_order: Option<f64>,
    assignee_ids: Option<Vec<Uuid>>,
    label_ids: Option<Vec<Uuid>>,
    module_ids: Option<Vec<Uuid>>,
    /// The `updated_at` the client's text was based on. The update is refused with 409 when
    /// someone else changed the title or description after it (other fields never conflict).
    expected_updated_at: Option<String>,
}

fn patch_from(req: UpdateReq) -> ApiResult<WorkItemPatch> {
    let description = match (req.description_json, req.description_html) {
        (None, None) => None,
        (j, h) => Some(richtext::clean_body(
            j.flatten(),
            h.as_deref().unwrap_or(""),
        )?),
    };
    if req.sort_order.is_some_and(|o| !o.is_finite()) {
        return Err(Error::bad_request("bad sort order").into());
    }
    let date = |d: Option<Option<String>>| -> ApiResult<Option<Option<String>>> {
        d.map(opt_date).transpose()
    };
    Ok(WorkItemPatch {
        title: req
            .title
            .as_deref()
            .map(work_item::clean_title)
            .transpose()?,
        description,
        state_id: req.state_id,
        priority: req.priority,
        start_date: date(req.start_date)?,
        due_date: date(req.due_date)?,
        estimate: req.estimate.map(|e| e.filter(|e| !e.is_empty())),
        parent_id: req.parent_id,
        cycle_id: req.cycle_id,
        sort_order: req.sort_order,
        assignee_ids: req.assignee_ids.map(dedup),
        label_ids: req.label_ids.map(dedup),
        module_ids: req.module_ids.map(dedup),
    })
}

fn dedup(mut v: Vec<Uuid>) -> Vec<Uuid> {
    let mut seen = std::collections::HashSet::new();
    v.retain(|x| seen.insert(*x));
    v
}

pub async fn update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<UpdateReq>,
) -> ApiResult<Json<WorkItem>> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    ctx.require_write(ProjAction::EditWorkItem)?;
    let before = load_item(&state, &ctx, id).await?;
    require_triager(&ctx, &before)?;
    let touches_text =
        req.title.is_some() || req.description_html.is_some() || req.description_json.is_some();
    if let Some(since) = req.expected_updated_at.as_deref().filter(|_| touches_text)
        && state.store.text_changed_by_others(id, since, me.id).await?
    {
        return Err(
            Error::conflict("someone else changed this text; reload to see their changes").into(),
        );
    }
    let p = patch_from(req)?;
    if p.is_empty() {
        return Ok(Json(load_item(&state, &ctx, id).await?));
    }
    let item = state.store.update_work_item(pid, id, &p, me.id).await?;
    // People newly @mentioned in the description hear about it.
    if let Some((_, html)) = &p.description {
        let old = super::collab::mentioned_ids(before.description_html.as_deref().unwrap_or(""));
        let new: Vec<Uuid> = super::collab::mentioned_ids(html)
            .into_iter()
            .filter(|u| !old.contains(u))
            .collect();
        if !new.is_empty() {
            for u in &new {
                if state.store.proj_role(pid, *u).await?.is_some() {
                    state.store.subscribe(id, *u).await?;
                }
            }
            let note = np_store::notifications::Note {
                kind: "mentioned",
                field: Some("description"),
                old: None,
                new: None,
                comment: None,
            };
            state.store.notify(id, me.id, &new, &note).await?;
        }
    }
    Ok(Json(item))
}

#[derive(Deserialize)]
pub struct BulkReq {
    ids: Vec<Uuid>,
    #[serde(flatten)]
    patch: UpdateReq,
}

/// The same change on many items (list view bulk edit). All or nothing per item: the
/// response lists the updated items; the first failure stops and is returned.
pub async fn bulk_update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Json(req): Json<BulkReq>,
) -> ApiResult<Json<Vec<WorkItem>>> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    ctx.require_write(ProjAction::EditWorkItem)?;
    if req.ids.is_empty() || req.ids.len() > 500 {
        return Err(Error::bad_request("bulk edit takes 1 to 500 work items").into());
    }
    let p = patch_from(req.patch)?;
    if p.title.is_some()
        || p.description.is_some()
        || p.parent_id.is_some()
        || p.sort_order.is_some()
    {
        return Err(Error::bad_request("bulk edit changes properties only").into());
    }
    let mut out = Vec::new();
    for id in dedup(req.ids) {
        out.push(state.store.update_work_item(pid, id, &p, me.id).await?);
    }
    Ok(Json(out))
}

pub async fn archive(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<WorkItem>> {
    set_archived(state, me, slug, pid, id, true).await
}

pub async fn unarchive(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<WorkItem>> {
    set_archived(state, me, slug, pid, id, false).await
}

async fn set_archived(
    state: AppState,
    me: np_core::User,
    slug: String,
    pid: Uuid,
    id: Uuid,
    on: bool,
) -> ApiResult<Json<WorkItem>> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    ctx.require_write(ProjAction::ArchiveWorkItem)?;
    load_item(&state, &ctx, id).await?;
    state
        .store
        .set_work_item_archived(pid, id, on, me.id)
        .await?;
    Ok(Json(load_item(&state, &ctx, id).await?))
}

pub async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    let item = load_item(&state, &ctx, id).await?;
    let own = if item.created_by == me.id {
        Ownership::Own
    } else {
        Ownership::Others
    };
    ctx.require_write(ProjAction::DeleteWorkItem(own))?;
    require_triager(&ctx, &item)?;
    state.store.delete_work_item(pid, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn activity(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Vec<Activity>>> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    load_item(&state, &ctx, id).await?;
    Ok(Json(state.store.work_item_activity(id).await?))
}

pub async fn relations(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<Vec<Relation>>> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    load_item(&state, &ctx, id).await?;
    Ok(Json(state.store.relations(id).await?))
}

#[derive(Deserialize)]
pub struct RelationReq {
    kind: RelationKind,
    work_item_id: Uuid,
}

pub async fn add_relation(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<RelationReq>,
) -> ApiResult<Json<Vec<Relation>>> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    ctx.require_write(ProjAction::EditWorkItem)?;
    load_item(&state, &ctx, id).await?;
    // The other item must be in a project the caller can see.
    let other_project = state
        .store
        .work_item_project(req.work_item_id)
        .await?
        .ok_or_else(|| Error::bad_request("related work item not found"))?;
    let other = load_project(&state, &slug, other_project, &me)
        .await
        .map_err(|_| Error::bad_request("related work item not found"))?;
    other
        .require(ProjAction::ViewWorkItems)
        .map_err(|_| Error::bad_request("related work item not found"))?;
    state
        .store
        .add_relation(ctx.ws.workspace.id, id, req.work_item_id, req.kind, me.id)
        .await?;
    Ok(Json(state.store.relations(id).await?))
}

pub async fn remove_relation(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id, rid)): Path<(String, Uuid, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    ctx.require_write(ProjAction::EditWorkItem)?;
    load_item(&state, &ctx, id).await?;
    state.store.remove_relation(id, rid, me.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Resolves `WEB-12` in a workspace to project and item ids (for links and search).
pub async fn by_key(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, key)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let ws = super::workspaces::load(&state, &slug, &me).await?;
    let (ident, seq) = key
        .rsplit_once('-')
        .and_then(|(i, s)| s.parse::<i64>().ok().map(|s| (i.to_uppercase(), s)))
        .ok_or_else(|| Error::bad_request("keys look like WEB-12"))?;
    let (pid, id) = state
        .store
        .work_item_by_key(ws.workspace.id, &ident, seq)
        .await?
        .ok_or_else(|| Error::not_found("work item not found"))?;
    let ctx = load_project(&state, &slug, pid, &me).await?;
    ctx.require(ProjAction::ViewWorkItems)
        .map_err(|_| Error::not_found("work item not found"))?;
    // Enough for a live reference card: title and state.
    let item = state
        .store
        .work_item(pid, id)
        .await?
        .ok_or_else(|| Error::not_found("work item not found"))?;
    let st = state
        .store
        .states(pid)
        .await?
        .into_iter()
        .find(|s| s.id == item.state_id);
    Ok(Json(serde_json::json!({
        "project_id": pid,
        "id": id,
        "key": format!("{}-{}", ctx.row.project.identifier, item.sequence),
        "title": item.title,
        "state": st.map(|s| serde_json::json!({ "name": s.name, "color": s.color, "group": s.group })),
        "archived": item.archived_at.is_some(),
    })))
}
