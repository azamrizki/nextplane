//! Projects, project members, states and labels (PERMISSIONS §2).

use crate::extract::Json;
use crate::extract::{Path, Query};
use axum::extract::State;
use axum::http::StatusCode;
use np_core::id::Uuid;
use np_core::perm::{self, ProjAction, ProjRole, WsAction, proj_require, ws_require};
use np_core::project::{self, EstimateType, StateGroup};
use np_core::{Error, User, user};
use np_store::projects::{
    Label, NewProject, ProjMember, ProjectPatch, ProjectRow, State as WfState,
};
use np_store::workspaces::MyWorkspace;
use serde::{Deserialize, Serialize};

use super::workspaces::{facts, load};
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;

/// A project as the caller sees it.
#[derive(Serialize)]
pub struct ProjectOut {
    #[serde(flatten)]
    row: ProjectRow,
    /// The role that applies (workspace owners/admins are always admin); `null` = not a member.
    my_role: Option<ProjRole>,
    is_member: bool,
}

pub struct ProjCtx {
    pub ws: MyWorkspace,
    pub row: ProjectRow,
    /// Effective role; `None` for a workspace member looking at a public project they have
    /// not joined (they may see it listed and join it, nothing more).
    pub role: Option<ProjRole>,
    /// Who is looking (for "own" rules such as drafts).
    pub viewer: Option<Uuid>,
}

impl ProjCtx {
    pub fn id(&self) -> Uuid {
        self.row.project.id
    }
    pub fn require(&self, action: ProjAction) -> ApiResult<()> {
        match self.role {
            None => Err(Error::forbidden("join this project first").into()),
            Some(r) => Ok(proj_require(Some(r), action)?),
        }
    }
    /// Any membership at all (to read members, states, labels).
    pub fn require_member(&self) -> ApiResult<ProjRole> {
        self.role
            .ok_or_else(|| Error::forbidden("join this project first").into())
    }
    fn out(self) -> ProjectOut {
        let is_member = self.row.member_role.0.is_some();
        ProjectOut {
            row: self.row,
            my_role: self.role,
            is_member,
        }
    }
}

/// Loads a project the user may see, or `404`.
pub async fn load_project(state: &AppState, slug: &str, id: Uuid, me: &User) -> ApiResult<ProjCtx> {
    let ws = load(state, slug, me).await?;
    let row = state
        .store
        .project_row(ws.workspace.id, id, me.id)
        .await?
        .ok_or_else(|| Error::not_found("project not found"))?;
    let explicit = row.member_role.0;
    if !perm::can_see_project(ws.role, explicit, row.project.is_private) {
        return Err(Error::not_found("project not found").into());
    }
    let role = perm::effective_proj_role(ws.role, explicit);
    Ok(ProjCtx {
        ws,
        row,
        role,
        viewer: Some(me.id),
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
    Path(slug): Path<String>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<ProjectOut>>> {
    let ws = load(&state, &slug, &me).await?;
    let rows = state
        .store
        .workspace_projects(ws.workspace.id, me.id, q.archived)
        .await?;
    Ok(Json(
        rows.into_iter()
            .filter(|r| perm::can_see_project(ws.role, r.member_role.0, r.project.is_private))
            .map(|row| {
                let role = perm::effective_proj_role(ws.role, row.member_role.0);
                ProjCtx {
                    ws: ws.clone(),
                    row,
                    role,
                    viewer: Some(me.id),
                }
                .out()
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    identifier: Option<String>,
    #[serde(default)]
    description: String,
    icon: Option<String>,
    cover_color: Option<String>,
    lead_id: Option<Uuid>,
    #[serde(default)]
    is_private: bool,
}

fn clean_name(raw: &str) -> ApiResult<String> {
    let n = user::normalize_name(raw, project::NAME_MAX, "project name")?;
    if n.is_empty() {
        return Err(Error::bad_request("project name is required").into());
    }
    Ok(n)
}

fn clean_description(raw: &str) -> ApiResult<String> {
    let d = raw.trim().to_string();
    if d.chars().count() > project::DESCRIPTION_MAX {
        return Err(Error::bad_request("description is too long").into());
    }
    Ok(d)
}

fn clean_icon(raw: Option<String>) -> ApiResult<Option<String>> {
    match raw.map(|i| i.trim().to_string()).filter(|i| !i.is_empty()) {
        Some(i) if i.chars().count() > 8 => Err(Error::bad_request("icon is one emoji").into()),
        other => Ok(other),
    }
}

async fn check_ws_member(state: &AppState, ws: Uuid, user: Option<Uuid>) -> ApiResult<()> {
    if let Some(u) = user
        && state.store.ws_role(ws, u).await?.is_none()
    {
        return Err(Error::bad_request("that person is not in this workspace").into());
    }
    Ok(())
}

pub async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<CreateReq>,
) -> ApiResult<(StatusCode, Json<ProjectOut>)> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::CreateProject, facts(&ws.workspace))?;
    let name = clean_name(&req.name)?;
    let identifier = project::validate_identifier(
        &req.identifier
            .unwrap_or_else(|| project::suggest_identifier(&name)),
    )?;
    let cover_color = req
        .cover_color
        .map(|c| project::validate_color(&c))
        .transpose()?;
    check_ws_member(&state, ws.workspace.id, req.lead_id).await?;
    let id = state
        .store
        .create_project(
            ws.workspace.id,
            &NewProject {
                identifier,
                name,
                description: clean_description(&req.description)?,
                icon: clean_icon(req.icon)?,
                cover_color,
                lead_id: req.lead_id,
                is_private: req.is_private,
            },
            me.id,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(load_project(&state, &slug, id, &me).await?.out()),
    ))
}

pub async fn get(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<ProjectOut>> {
    Ok(Json(load_project(&state, &slug, id, &me).await?.out()))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    identifier: Option<String>,
    description: Option<String>,
    #[serde(default, with = "crate::util::double_option")]
    icon: Option<Option<String>>,
    #[serde(default, with = "crate::util::double_option")]
    cover_color: Option<Option<String>>,
    #[serde(default, with = "crate::util::double_option")]
    lead_id: Option<Option<Uuid>>,
    #[serde(default, with = "crate::util::double_option")]
    default_assignee_id: Option<Option<Uuid>>,
    is_private: Option<bool>,
    estimate_type: Option<EstimateType>,
    estimate_values: Option<Vec<String>>,
    feature_cycles: Option<bool>,
    feature_modules: Option<bool>,
    feature_pages: Option<bool>,
    feature_intake: Option<bool>,
    /// An uploaded file (`/api/v1/files/{id}`) or an Unsplash photo; `null` removes it.
    #[serde(default, with = "crate::util::double_option")]
    cover_image: Option<Option<String>>,
    /// The photographer of an Unsplash cover.
    cover_credit: Option<CoverCredit>,
    /// Unsplash's download link of the chosen photo: Unsplash is told it was used.
    unsplash_download: Option<String>,
    /// Automations, in months (0 = off, at most 12).
    archive_in: Option<i64>,
    close_in: Option<i64>,
    #[serde(default, with = "crate::util::double_option")]
    close_state_id: Option<Option<Uuid>>,
}

pub async fn update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
    Json(req): Json<UpdateReq>,
) -> ApiResult<Json<ProjectOut>> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require(ProjAction::EditSettings)?;
    let ws_id = ctx.ws.workspace.id;
    let mut p = ProjectPatch {
        name: req.name.as_deref().map(clean_name).transpose()?,
        identifier: req
            .identifier
            .as_deref()
            .map(project::validate_identifier)
            .transpose()?,
        description: req
            .description
            .as_deref()
            .map(clean_description)
            .transpose()?,
        is_private: req.is_private,
        feature_cycles: req.feature_cycles,
        feature_modules: req.feature_modules,
        feature_pages: req.feature_pages,
        feature_intake: req.feature_intake,
        ..Default::default()
    };
    automations(&state, &ctx, &req, &mut p).await?;
    cover(&state, &req, &mut p).await?;
    if let Some(i) = req.icon {
        p.icon = Some(clean_icon(i)?);
    }
    if let Some(c) = req.cover_color {
        p.cover_color = Some(c.map(|c| project::validate_color(&c)).transpose()?);
    }
    if let Some(l) = req.lead_id {
        check_ws_member(&state, ws_id, l).await?;
        p.lead_id = Some(l);
    }
    if let Some(a) = req.default_assignee_id {
        if let Some(u) = a
            && state.store.proj_role(id, u).await?.is_none()
        {
            return Err(Error::bad_request("the default assignee must be a project member").into());
        }
        p.default_assignee_id = Some(a);
    }
    if req.estimate_type.is_some() || req.estimate_values.is_some() {
        let t = req.estimate_type.unwrap_or(ctx.row.project.estimate_type.0);
        let vals = req.estimate_values.unwrap_or_else(|| t.default_values());
        p.estimate = Some((t, project::validate_estimates(t, &vals)?));
    }
    state.store.update_project(id, &p).await?;
    Ok(Json(load_project(&state, &slug, id, &me).await?.out()))
}

#[derive(Deserialize, serde::Serialize)]
pub struct CoverCredit {
    name: String,
    url: String,
}

/// A cover is one of our own files or an Unsplash photo (when Unsplash is set up), and an
/// Unsplash photo carries its photographer's credit (Unsplash's API guidelines).
async fn cover(state: &AppState, req: &UpdateReq, p: &mut ProjectPatch) -> ApiResult<()> {
    let Some(img) = &req.cover_image else {
        return Ok(());
    };
    let Some(url) = img.as_deref().map(str::trim).filter(|u| !u.is_empty()) else {
        p.cover_image = Some(None);
        return Ok(());
    };
    let own = url
        .strip_prefix("/api/v1/files/")
        .is_some_and(|id| id.parse::<Uuid>().is_ok());
    let unsplash = url.starts_with("https://images.unsplash.com/")
        && url.len() <= 2048
        && !url.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'' || c == '<');
    if !own && !unsplash {
        return Err(Error::bad_request("a cover is an uploaded image or an Unsplash photo").into());
    }
    let mut credit = None;
    if unsplash {
        let key = crate::services::unsplash_key(&state.store).await?;
        if key.is_empty() {
            return Err(Error::bad_request("Unsplash is not set up on this server").into());
        }
        let c = req
            .cover_credit
            .as_ref()
            .ok_or_else(|| Error::bad_request("an Unsplash photo needs its photographer's credit"))?;
        if c.name.trim().is_empty() || !c.url.starts_with("https://unsplash.com/") || c.url.len() > 500 {
            return Err(Error::bad_request("the photographer's credit is a name and an unsplash.com link").into());
        }
        credit = Some(
            serde_json::json!({ "name": c.name.trim().chars().take(100).collect::<String>(), "url": c.url })
                .to_string(),
        );
        if let Some(d) = req.unsplash_download.clone() {
            let (http, base) = (state.http.clone(), state.unsplash_api.clone());
            tokio::spawn(async move { crate::services::unsplash_track(&http, &base, &key, &d).await });
        }
    }
    p.cover_image = Some(Some((url.to_string(), credit)));
    Ok(())
}

/// Automation settings (as in Plane): months 0–12; auto-close needs a cancelled state of this
/// project, the first one when none is given, and turning it off forgets the state.
async fn automations(
    state: &AppState,
    ctx: &ProjCtx,
    req: &UpdateReq,
    p: &mut ProjectPatch,
) -> ApiResult<()> {
    for m in [req.archive_in, req.close_in].into_iter().flatten() {
        if !(0..=12).contains(&m) {
            return Err(Error::bad_request("automations take 0 to 12 months").into());
        }
    }
    p.archive_in = req.archive_in;
    p.close_in = req.close_in;
    let close_in = req.close_in.unwrap_or(ctx.row.project.close_in);
    if close_in == 0 {
        if req.close_in.is_some() || req.close_state_id.is_some() {
            p.close_state_id = Some(None);
        }
        return Ok(());
    }
    let wanted = match req.close_state_id {
        Some(Some(s)) => Some(s),
        Some(None) => None,
        None => ctx.row.project.close_state_id,
    };
    let mut cancelled: Vec<_> = state
        .store
        .states(ctx.id())
        .await?
        .into_iter()
        .filter(|s| s.group.0 == StateGroup::Cancelled)
        .collect();
    cancelled.sort_by(|a, b| a.sort_order.total_cmp(&b.sort_order));
    let target = match wanted {
        Some(s) if cancelled.iter().any(|c| c.id == s) => s,
        Some(_) => {
            return Err(Error::bad_request(
                "auto-close moves items to a cancelled state of this project",
            )
            .into());
        }
        None => match cancelled.first() {
            Some(c) => c.id,
            None => {
                return Err(Error::bad_request(
                    "this project has no cancelled state to close work items into",
                )
                .into());
            }
        },
    };
    p.close_state_id = Some(Some(target));
    Ok(())
}

pub async fn archive(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<ProjectOut>> {
    set_archived(state, me, slug, id, true).await
}

pub async fn unarchive(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<ProjectOut>> {
    set_archived(state, me, slug, id, false).await
}

async fn set_archived(
    state: AppState,
    me: User,
    slug: String,
    id: Uuid,
    on: bool,
) -> ApiResult<Json<ProjectOut>> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require(ProjAction::EditSettings)?;
    state.store.set_project_archived(id, on).await?;
    Ok(Json(load_project(&state, &slug, id, &me).await?.out()))
}

pub async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require(ProjAction::EditSettings)?;
    state.store.delete_project(id).await?;
    tracing::info!(project = %ctx.row.project.identifier, by = %me.email, "project deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// Joins a public project (PERMISSIONS §2, joining).
pub async fn join(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<ProjectOut>> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    if ctx.row.member_role.0.is_none() {
        if ctx.row.project.is_private {
            return Err(Error::forbidden("private projects are joined by invitation").into());
        }
        let role = ProjRole::on_self_join(ctx.ws.role)
            .ok_or_else(|| Error::forbidden("guests cannot join projects on their own"))?;
        state.store.set_project_member(id, me.id, role).await?;
    }
    Ok(Json(load_project(&state, &slug, id, &me).await?.out()))
}

// ---- members ----------------------------------------------------------------------------

pub async fn members(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<Vec<ProjMember>>> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require_member()?;
    Ok(Json(
        state.store.project_members(id, ctx.ws.workspace.id).await?,
    ))
}

#[derive(Deserialize)]
pub struct AddMemberReq {
    user_id: Uuid,
    role: ProjRole,
}

pub async fn add_member(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
    Json(req): Json<AddMemberReq>,
) -> ApiResult<StatusCode> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    set_member(&state, &ctx, req.user_id, req.role).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct RoleReq {
    role: ProjRole,
}

pub async fn set_member_role(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id, user_id)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<RoleReq>,
) -> ApiResult<StatusCode> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    if state.store.proj_role(id, user_id).await?.is_none() {
        return Err(Error::not_found("member not found").into());
    }
    set_member(&state, &ctx, user_id, req.role).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_member(
    state: &AppState,
    ctx: &ProjCtx,
    user_id: Uuid,
    role: ProjRole,
) -> ApiResult<()> {
    let actor = ctx.require_member()?;
    let target_ws = state
        .store
        .ws_role(ctx.ws.workspace.id, user_id)
        .await?
        .ok_or_else(|| Error::bad_request("that person is not in this workspace"))?;
    let from = state.store.proj_role(ctx.id(), user_id).await?;
    let admins = state.store.count_project_admins(ctx.id()).await?;
    perm::can_set_proj_role(actor, target_ws, from, role, admins)?;
    state
        .store
        .set_project_member(ctx.id(), user_id, role)
        .await?;
    Ok(())
}

pub async fn remove_member(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id, user_id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    end_member(&state, &ctx, user_id, user_id == me.id).await
}

pub async fn leave(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    end_member(&state, &ctx, me.id, true).await
}

async fn end_member(
    state: &AppState,
    ctx: &ProjCtx,
    user_id: Uuid,
    is_self: bool,
) -> ApiResult<StatusCode> {
    let actor = ctx.require_member()?;
    let target = state
        .store
        .proj_role(ctx.id(), user_id)
        .await?
        .ok_or_else(|| Error::not_found("member not found"))?;
    let admins = state.store.count_project_admins(ctx.id()).await?;
    perm::can_remove_proj_member(actor, target, is_self, admins)?;
    state.store.remove_project_member(ctx.id(), user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- states -----------------------------------------------------------------------------

pub async fn states(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<Vec<WfState>>> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require_member()?;
    Ok(Json(state.store.states(id).await?))
}

fn clean_short_name(raw: &str, what: &str) -> ApiResult<String> {
    let n = user::normalize_name(raw, 50, what)?;
    if n.is_empty() {
        return Err(Error::bad_request(format!("{what} is required")).into());
    }
    Ok(n)
}

#[derive(Deserialize)]
pub struct StateReq {
    name: String,
    color: String,
    group: StateGroup,
}

pub async fn create_state(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
    Json(req): Json<StateReq>,
) -> ApiResult<(StatusCode, Json<WfState>)> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require(ProjAction::ManageStatesLabelsEstimates)?;
    let s = state
        .store
        .create_state(
            id,
            &clean_short_name(&req.name, "state name")?,
            &project::validate_color(&req.color)?,
            req.group,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(s)))
}

#[derive(Deserialize)]
pub struct StatePatch {
    name: Option<String>,
    color: Option<String>,
    group: Option<StateGroup>,
    sort_order: Option<f64>,
    #[serde(default)]
    is_default: bool,
}

pub async fn update_state(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id, state_id)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<StatePatch>,
) -> ApiResult<Json<Vec<WfState>>> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require(ProjAction::ManageStatesLabelsEstimates)?;
    let name = req
        .name
        .as_deref()
        .map(|n| clean_short_name(n, "state name"))
        .transpose()?;
    let color = req
        .color
        .as_deref()
        .map(project::validate_color)
        .transpose()?;
    if req.sort_order.is_some_and(|o| !o.is_finite()) {
        return Err(Error::bad_request("bad sort order").into());
    }
    state
        .store
        .update_state(
            id,
            state_id,
            name.as_deref(),
            color.as_deref(),
            req.group,
            req.sort_order,
            req.is_default,
        )
        .await?;
    Ok(Json(state.store.states(id).await?))
}

pub async fn delete_state(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id, state_id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require(ProjAction::ManageStatesLabelsEstimates)?;
    state.store.delete_state(id, state_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- labels -----------------------------------------------------------------------------

pub async fn labels(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<Vec<Label>>> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require_member()?;
    Ok(Json(state.store.labels(id).await?))
}

#[derive(Deserialize)]
pub struct LabelReq {
    name: String,
    color: String,
    parent_id: Option<Uuid>,
}

pub async fn create_label(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
    Json(req): Json<LabelReq>,
) -> ApiResult<(StatusCode, Json<Label>)> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require(ProjAction::ManageStatesLabelsEstimates)?;
    let l = state
        .store
        .create_label(
            id,
            &clean_short_name(&req.name, "label name")?,
            &project::validate_color(&req.color)?,
            req.parent_id,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(l)))
}

#[derive(Deserialize)]
pub struct LabelPatch {
    name: Option<String>,
    color: Option<String>,
    #[serde(default, with = "crate::util::double_option")]
    parent_id: Option<Option<Uuid>>,
    sort_order: Option<f64>,
}

pub async fn update_label(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id, label_id)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<LabelPatch>,
) -> ApiResult<Json<Vec<Label>>> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require(ProjAction::ManageStatesLabelsEstimates)?;
    let name = req
        .name
        .as_deref()
        .map(|n| clean_short_name(n, "label name"))
        .transpose()?;
    let color = req
        .color
        .as_deref()
        .map(project::validate_color)
        .transpose()?;
    if req.sort_order.is_some_and(|o| !o.is_finite()) {
        return Err(Error::bad_request("bad sort order").into());
    }
    state
        .store
        .update_label(
            id,
            label_id,
            name.as_deref(),
            color.as_deref(),
            req.parent_id,
            req.sort_order,
        )
        .await?;
    Ok(Json(state.store.labels(id).await?))
}

pub async fn delete_label(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id, label_id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let ctx = load_project(&state, &slug, id, &me).await?;
    ctx.require(ProjAction::ManageStatesLabelsEstimates)?;
    state.store.delete_label(id, label_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
