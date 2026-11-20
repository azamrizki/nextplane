//! Intake (SPRINT S3.3, PERMISSIONS §2): everyone in a project submits; guests see only
//! their own submissions; submitters edit or withdraw theirs until it is triaged; project
//! admins accept, decline, mark duplicates and snooze. An optional public form lets people
//! without an account submit through a link.

use axum::extract::State;
use axum::http::StatusCode;
use np_core::id::Uuid;
use np_core::perm::{Ownership, ProjAction, ProjRole};
use np_core::work_item::{self, Priority};
use np_core::{Error, User, user};
use np_store::intake::{IntakeEntry, IntakeView, Submission};
use np_store::work_items::WorkItemPatch;
use serde::{Deserialize, Serialize};

use super::projects::{ProjCtx, load_project};
use crate::AppState;
use crate::auth::{ClientIp, CurrentUser};
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};
use crate::ratelimit::Limit;
use crate::richtext;

const FORM_PER_IP: Limit = Limit {
    max: 5,
    window: std::time::Duration::from_secs(10 * 60),
};
const FORM_PER_LINK: Limit = Limit {
    max: 60,
    window: std::time::Duration::from_secs(60 * 60),
};

async fn ctx(state: &AppState, me: &User, slug: &str, pid: Uuid) -> ApiResult<ProjCtx> {
    let c = load_project(state, slug, pid, me).await?;
    c.require(ProjAction::SubmitIntake)?;
    if !c.row.project.feature_intake {
        return Err(Error::forbidden("intake is turned off for this project").into());
    }
    Ok(c)
}

fn is_admin(c: &ProjCtx) -> bool {
    c.role == Some(ProjRole::Admin)
}

/// Guests see only what they submitted.
fn only_mine(c: &ProjCtx, me: &User) -> Option<Uuid> {
    c.role
        .is_none_or(|r| r < ProjRole::Commenter)
        .then_some(me.id)
}

/// Submitter emails are for the people who triage.
fn redact(c: &ProjCtx, mut e: IntakeEntry) -> IntakeEntry {
    if !is_admin(c) {
        e.submitter_email = None;
    }
    e
}

async fn entry(state: &AppState, c: &ProjCtx, me: &User, id: Uuid) -> ApiResult<IntakeEntry> {
    let e = state
        .store
        .intake_entry(c.id(), id)
        .await?
        .ok_or_else(|| Error::not_found("intake submission not found"))?;
    if only_mine(c, me).is_some_and(|u| u != e.created_by) {
        return Err(Error::not_found("intake submission not found").into());
    }
    Ok(e)
}

#[derive(Deserialize)]
pub struct ListQuery {
    view: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<IntakeEntry>>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let view = IntakeView::parse(q.view.as_deref().unwrap_or("open"))
        .ok_or_else(|| Error::bad_request("view is open, snoozed, closed or all"))?;
    let rows = state
        .store
        .intake(pid, view, only_mine(&c, &me), 500)
        .await?;
    Ok(Json(rows.into_iter().map(|e| redact(&c, e)).collect()))
}

#[derive(Serialize)]
pub struct Count {
    open: i64,
}

pub async fn count(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
) -> ApiResult<Json<Count>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    Ok(Json(Count {
        open: state
            .store
            .intake_open_count(pid, only_mine(&c, &me))
            .await?,
    }))
}

#[derive(Deserialize)]
pub struct SubmitReq {
    title: String,
    #[serde(default)]
    description_html: String,
    priority: Option<String>,
}

fn clean_title(t: &str) -> ApiResult<String> {
    let t = user::normalize_name(t, 255, "title")?;
    if t.is_empty() {
        return Err(Error::bad_request("give a title").into());
    }
    Ok(t)
}

pub async fn submit(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Json(req): Json<SubmitReq>,
) -> ApiResult<(StatusCode, Json<IntakeEntry>)> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::SubmitIntake)?;
    let title = clean_title(&req.title)?;
    let (_, html) = richtext::clean_body(None, &req.description_html)?;
    let priority = req.priority.as_deref().map(Priority::parse).transpose()?;
    let item = state
        .store
        .submit_intake(
            pid,
            me.id,
            &Submission {
                title: &title,
                description_html: &html,
                priority,
                source: "app",
                submitter_name: None,
                submitter_email: None,
            },
        )
        .await?;
    let e = state
        .store
        .intake_entry(pid, item.id)
        .await?
        .ok_or_else(|| Error::not_found("intake submission not found"))?;
    Ok((StatusCode::CREATED, Json(redact(&c, e))))
}

pub async fn get(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<IntakeEntry>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    Ok(Json(redact(&c, entry(&state, &c, &me, id).await?)))
}

/// Submitters change their own until it is triaged; admins until it is accepted.
fn may_edit(c: &ProjCtx, me: &User, e: &IntakeEntry) -> bool {
    let own = if e.created_by == me.id {
        Ownership::Own
    } else {
        Ownership::Others
    };
    let open = matches!(e.status.as_str(), "pending" | "snoozed");
    c.row.project.archived_at.is_none()
        && e.status != "accepted"
        && c.role
            .is_some_and(|r| np_core::perm::proj_can(r, ProjAction::EditIntake(own)))
        && (is_admin(c) || open)
}

#[derive(Deserialize)]
pub struct EditReq {
    title: Option<String>,
    description_html: Option<String>,
}

pub async fn edit(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
    Json(req): Json<EditReq>,
) -> ApiResult<Json<IntakeEntry>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let e = entry(&state, &c, &me, id).await?;
    if !may_edit(&c, &me, &e) {
        return Err(Error::forbidden("you cannot change this submission").into());
    }
    let patch = WorkItemPatch {
        title: req.title.as_deref().map(clean_title).transpose()?,
        description: req
            .description_html
            .as_deref()
            .map(|h| richtext::clean_body(None, h))
            .transpose()?,
        ..Default::default()
    };
    if !patch.is_empty() {
        state.store.update_work_item(pid, id, &patch, me.id).await?;
    }
    Ok(Json(redact(&c, entry(&state, &c, &me, id).await?)))
}

pub async fn withdraw(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let c = ctx(&state, &me, &slug, pid).await?;
    let e = entry(&state, &c, &me, id).await?;
    if !may_edit(&c, &me, &e) {
        return Err(Error::forbidden("you cannot delete this submission").into());
    }
    state.store.delete_work_item(pid, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Default)]
pub struct TriageReq {
    reason: Option<String>,
    /// The work item key (`WEB-12`) it duplicates.
    duplicate_of: Option<String>,
    /// A date (`YYYY-MM-DD`, start of that day UTC) or a timestamp.
    until: Option<String>,
}

#[derive(Deserialize)]
pub struct TriagePath {
    slug: String,
    id: Uuid,
    item: Uuid,
    action: String,
}

pub async fn triage(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(tp): Path<TriagePath>,
    body: axum::body::Bytes,
) -> ApiResult<Json<IntakeEntry>> {
    let c = ctx(&state, &me, &tp.slug, tp.id).await?;
    c.require_write(ProjAction::TriageIntake)?;
    let e = entry(&state, &c, &me, tp.item).await?;
    let trimmed = std::str::from_utf8(&body).unwrap_or("").trim();
    let req: TriageReq = if trimmed.is_empty() || trimmed == "null" {
        TriageReq::default()
    } else {
        serde_json::from_slice(&body).map_err(|err| Error::bad_request(err.to_string()))?
    };
    let reason = req
        .reason
        .as_deref()
        .map(|r| user::normalize_name(r, 1000, "reason"))
        .transpose()?
        .filter(|r| !r.is_empty());
    let dup = match (tp.action.as_str(), req.duplicate_of.as_deref()) {
        ("duplicate", Some(key)) => {
            let (ident, seq) = key
                .trim()
                .rsplit_once('-')
                .and_then(|(i, s)| s.parse::<i64>().ok().map(|s| (i.to_uppercase(), s)))
                .ok_or_else(|| Error::bad_request("keys look like WEB-12"))?;
            let (p, id) = state
                .store
                .work_item_by_key(c.ws.workspace.id, &ident, seq)
                .await?
                .filter(|(p, _)| *p == tp.id)
                .ok_or_else(|| {
                    Error::bad_request("the duplicate must be a work item of this project")
                })?;
            let _ = p;
            Some(id)
        }
        ("duplicate", None) => {
            return Err(Error::bad_request("say which work item it duplicates").into());
        }
        _ => None,
    };
    let until = match (tp.action.as_str(), req.until) {
        ("snooze", Some(u)) => {
            let ts = if u.len() == 10 {
                format!("{}T00:00:00.000000Z", work_item::validate_date(&u)?)
            } else {
                u
            };
            let t = np_store::parse_ts(&ts)
                .ok_or_else(|| Error::bad_request("snooze until a time in the future"))?;
            Some(np_store::time_fmt::fmt_ts(t))
        }
        ("snooze", None) => return Err(Error::bad_request("snooze needs \"until\"").into()),
        _ => None,
    };
    let out = state
        .store
        .triage(
            tp.id,
            tp.item,
            me.id,
            &tp.action,
            reason.as_deref(),
            dup,
            until.as_deref(),
        )
        .await?;
    // People who used the public form hear back by email.
    if let (Some(mailer), Some(email), "form") = (
        &state.mailer,
        e.submitter_email.as_deref(),
        e.source.as_str(),
    ) {
        let what = match out.status.as_str() {
            "accepted" => "was accepted".to_string(),
            "declined" => match &out.reason {
                Some(r) => format!("was declined: {r}"),
                None => "was declined".to_string(),
            },
            "duplicate" => "is already being tracked, so it was closed as a duplicate".to_string(),
            _ => String::new(),
        };
        if !what.is_empty() {
            let project = &c.row.project.name;
            mailer.send_later(
                email,
                format!("Your request to {project}: {}", e.title),
                format!("Hello,\n\nYour request \"{}\" to {project} {what}.\n\nThank you for writing in.\n", e.title),
            );
        }
    }
    Ok(Json(redact(&c, out)))
}

// ---- public form -----------------------------------------------------------------------

#[derive(Serialize)]
pub struct FormSettings {
    enabled: bool,
    /// Path of the public page (`/intake/<token>`).
    path: Option<String>,
}

pub async fn form_settings(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
) -> ApiResult<Json<FormSettings>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require(ProjAction::TriageIntake)?;
    let t = state.store.intake_form_token(pid).await?;
    Ok(Json(FormSettings {
        enabled: t.is_some(),
        path: t.map(|t| format!("/intake/{t}")),
    }))
}

#[derive(Deserialize)]
pub struct FormReq {
    enabled: bool,
}

/// Turning it on always makes a new link; the old one stops working.
pub async fn set_form(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Json(req): Json<FormReq>,
) -> ApiResult<Json<FormSettings>> {
    let c = ctx(&state, &me, &slug, pid).await?;
    c.require_write(ProjAction::TriageIntake)?;
    let token = req
        .enabled
        .then(|| crate::auth::random_token().replace(['-', '_'], "x"));
    state.store.set_intake_form(pid, token.as_deref()).await?;
    Ok(Json(FormSettings {
        enabled: token.is_some(),
        path: token.map(|t| format!("/intake/{t}")),
    }))
}

#[derive(Serialize)]
pub struct PublicForm {
    workspace: String,
    project: String,
}

fn form_token(t: &str) -> ApiResult<&str> {
    if t.is_empty() || t.len() > 128 || !t.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::not_found("this form does not exist or was turned off").into());
    }
    Ok(t)
}

pub async fn public_form(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<PublicForm>> {
    let (_, workspace, _, project) = state
        .store
        .intake_form(form_token(&token)?)
        .await?
        .ok_or_else(|| Error::not_found("this form does not exist or was turned off"))?;
    Ok(Json(PublicForm { workspace, project }))
}

#[derive(Deserialize)]
pub struct PublicSubmit {
    #[serde(default)]
    name: String,
    #[serde(default)]
    email: String,
    title: String,
    #[serde(default)]
    description: String,
    /// A field people do not see; bots fill it in.
    #[serde(default)]
    website: String,
}

pub async fn public_submit(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Path(token): Path<String>,
    Json(req): Json<PublicSubmit>,
) -> ApiResult<StatusCode> {
    let token = form_token(&token)?;
    state
        .limiter
        .check(&format!("intake-form-ip:{ip}"), FORM_PER_IP)?;
    let (ws, _, pid, _) = state
        .store
        .intake_form(token)
        .await?
        .ok_or_else(|| Error::not_found("this form does not exist or was turned off"))?;
    state
        .limiter
        .check(&format!("intake-form:{pid}"), FORM_PER_LINK)?;
    if !req.website.is_empty() {
        // Looks like a bot: pretend it worked.
        return Ok(StatusCode::ACCEPTED);
    }
    let title = clean_title(&req.title)?;
    let name = user::normalize_name(&req.name, 120, "name")?;
    let email = req.email.trim().to_lowercase();
    if !email.is_empty() {
        user::normalize_email(&email)?;
    }
    if req.description.len() > 20_000 {
        return Err(Error::bad_request("the description is too long").into());
    }
    let html = crate::mcp::text_to_html(&req.description);
    let bot = state.store.intake_form_user(ws).await?;
    state
        .store
        .submit_intake(
            pid,
            bot,
            &Submission {
                title: &title,
                description_html: &html,
                priority: None,
                source: "form",
                submitter_name: (!name.is_empty()).then_some(name.as_str()),
                submitter_email: (!email.is_empty()).then_some(email.as_str()),
            },
        )
        .await?;
    Ok(StatusCode::ACCEPTED)
}
