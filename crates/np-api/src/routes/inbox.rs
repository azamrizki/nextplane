//! The inbox (notifications), email preference, drafts and the email digest.

use axum::extract::State;
use axum::http::StatusCode;
use np_core::id::Uuid;
use np_core::perm::{self, ProjAction};
use np_core::{Error, work_item};
use np_store::Store;
use np_store::notifications::Notification;
use np_store::views::WorkItemInProject;
use serde::Deserialize;

use super::projects::load_project;
use super::workspaces::load;
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};
use crate::mail::Mailer;

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "unread")]
    filter: String,
    limit: Option<i64>,
}

fn unread() -> String {
    "unread".into()
}

pub async fn list(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Notification>>> {
    let ws = load(&state, &slug, &me).await?;
    Ok(Json(
        state
            .store
            .notifications(
                me.id,
                ws.workspace.id,
                &q.filter,
                q.limit.unwrap_or(100).clamp(1, 500),
            )
            .await?,
    ))
}

pub async fn count(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let ws = load(&state, &slug, &me).await?;
    Ok(Json(
        serde_json::json!({ "unread": state.store.unread_count(me.id, ws.workspace.id).await? }),
    ))
}

#[derive(Deserialize, Default)]
pub struct MarkReq {
    until: Option<String>,
}

pub async fn mark(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id, action)): Path<(String, Uuid, String)>,
    body: axum::body::Bytes,
) -> ApiResult<StatusCode> {
    load(&state, &slug, &me).await?;
    // The body is optional: only `snooze` needs one.
    let trimmed = std::str::from_utf8(&body).unwrap_or("").trim();
    let until = if trimmed.is_empty() || trimmed == "null" {
        None
    } else {
        serde_json::from_slice::<MarkReq>(&body)
            .map_err(|e| Error::bad_request(e.to_string()))?
            .until
    };
    let until = match (action.as_str(), until) {
        ("snooze", Some(u)) => {
            // A date (start of that day, UTC) or a full timestamp.
            let ts = if u.len() == 10 {
                format!("{}T00:00:00.000000Z", work_item::validate_date(&u)?)
            } else {
                u
            };
            if np_store::parse_ts(&ts).is_none() || ts <= np_store::now_ts() {
                return Err(Error::bad_request("snooze until a time in the future").into());
            }
            Some(ts)
        }
        ("snooze", None) => return Err(Error::bad_request("snooze needs \"until\"").into()),
        (_, _) => None,
    };
    state
        .store
        .mark_notification(me.id, id, &action, until.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn read_all(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let ws = load(&state, &slug, &me).await?;
    Ok(Json(
        serde_json::json!({ "marked": state.store.read_all(me.id, ws.workspace.id).await? }),
    ))
}

#[derive(Deserialize)]
pub struct Prefs {
    email_notifications: Option<bool>,
}

pub async fn get_prefs(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(
        serde_json::json!({ "email_notifications": state.store.email_notifications(me.id).await?, "email_enabled": state.mailer.is_some() }),
    ))
}

pub async fn set_prefs(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(p): Json<Prefs>,
) -> ApiResult<Json<serde_json::Value>> {
    if let Some(on) = p.email_notifications {
        state.store.set_email_notifications(me.id, on).await?;
    }
    get_prefs(State(state), CurrentUser(me)).await
}

/// My drafts across the projects I can write in.
pub async fn drafts(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<WorkItemInProject>>> {
    let ws = load(&state, &slug, &me).await?;
    let mut out = Vec::new();
    for r in state
        .store
        .workspace_projects(ws.workspace.id, me.id, false)
        .await?
    {
        let role = perm::effective_proj_role(ws.role, r.member_role.0);
        if !role.is_some_and(|x| perm::proj_can(x, ProjAction::CreateWorkItem)) {
            continue;
        }
        let f = np_store::work_items::Filter {
            drafts_of: Some(me.id),
            limit: Some(500),
            ..Default::default()
        };
        for item in state.store.work_items(r.project.id, &f).await? {
            out.push(WorkItemInProject {
                item,
                identifier: r.project.identifier.clone(),
                project_name: r.project.name.clone(),
            });
        }
    }
    out.sort_by(|a, b| b.item.updated_at.cmp(&a.item.updated_at));
    Ok(Json(out))
}

pub async fn publish(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<np_store::work_items::WorkItem>> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    ctx.require_write(ProjAction::CreateWorkItem)?;
    let item = state
        .store
        .work_item(pid, wid)
        .await?
        .filter(|w| w.is_draft && w.created_by == me.id)
        .ok_or_else(|| Error::not_found("draft not found"))?;
    state.store.publish_draft(pid, item.id, me.id).await?;
    Ok(Json(
        state
            .store
            .work_item(pid, wid)
            .await?
            .ok_or_else(|| Error::not_found("work item not found"))?,
    ))
}

fn describe(n: &Notification) -> String {
    match n.kind.as_str() {
        "assigned" => "assigned you".into(),
        "mentioned" => "mentioned you".into(),
        "commented" => "commented".into(),
        "archived" => "archived it".into(),
        _ => format!(
            "changed {}",
            n.field.as_deref().unwrap_or("it").replace('_', " ")
        ),
    }
}

/// One email per person with their unread notifications older than ten minutes (so a
/// burst of edits becomes one message). Returns how many emails went out.
pub async fn send_digests(store: &Store, mailer: &Mailer, base: &str) -> anyhow::Result<usize> {
    let cutoff = np_store::fmt_ts(time::OffsetDateTime::now_utc() - time::Duration::minutes(10));
    let batches = store
        .digest_batches(&cutoff)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let n = batches.len();
    for (email, name, ns) in batches {
        let mut body = format!(
            "Hi {},\n\nNew activity on work items you follow:\n\n",
            if name.is_empty() { &email } else { &name }
        );
        for x in &ns {
            body.push_str(&format!(
                "• {}-{} {} — {}\n",
                x.identifier,
                x.sequence,
                x.title,
                describe(x)
            ));
        }
        body.push_str(&format!(
            "\nOpen your inbox: {base}\nTurn these emails off in your profile.\n"
        ));
        mailer.send_later(
            &email,
            format!(
                "NextPlane: {} new notification{}",
                ns.len(),
                if ns.len() == 1 { "" } else { "s" }
            ),
            body,
        );
        store
            .mark_emailed(&ns.iter().map(|x| x.id).collect::<Vec<_>>())
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
    }
    Ok(n)
}
