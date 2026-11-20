//! Workspace members and invites (PERMISSIONS §1).

use crate::extract::Json;
use crate::extract::Path;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use np_core::Error;
use np_core::id::Uuid;
use np_core::perm::{self, WsAction, WsRole, ws_require};
use np_core::user;
use np_store::invites::Invite;
use np_store::workspaces::WsMember;
use serde::{Deserialize, Serialize};
use time::Duration;

use super::workspaces::{facts, load};
use crate::AppState;
use crate::auth::{self, CurrentUser};
use crate::error::ApiResult;

pub const INVITE_TTL: Duration = Duration::days(7);

pub async fn list(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<WsMember>>> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::ViewMembersPage, facts(&ws.workspace))?;
    Ok(Json(state.store.workspace_members(ws.workspace.id).await?))
}

#[derive(Deserialize)]
pub struct RoleReq {
    role: WsRole,
}

pub async fn set_role(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, user_id)): Path<(String, Uuid)>,
    Json(req): Json<RoleReq>,
) -> ApiResult<StatusCode> {
    let ws = load(&state, &slug, &me).await?;
    let id = ws.workspace.id;
    let from = state
        .store
        .ws_role(id, user_id)
        .await?
        .ok_or_else(|| Error::not_found("member not found"))?;
    let owners = state.store.count_owners(id).await?;
    perm::can_change_role(ws.role, from, req.role, user_id == me.id, owners)?;
    state.store.set_ws_role(id, user_id, req.role).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, user_id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    let ws = load(&state, &slug, &me).await?;
    end_membership(&state, ws.workspace.id, ws.role, user_id, user_id == me.id).await
}

pub async fn leave(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<StatusCode> {
    let ws = load(&state, &slug, &me).await?;
    end_membership(&state, ws.workspace.id, ws.role, me.id, true).await
}

async fn end_membership(
    state: &AppState,
    ws: Uuid,
    actor: WsRole,
    user_id: Uuid,
    is_self: bool,
) -> ApiResult<StatusCode> {
    let target = state
        .store
        .ws_role(ws, user_id)
        .await?
        .ok_or_else(|| Error::not_found("member not found"))?;
    let owners = state.store.count_owners(ws).await?;
    perm::can_remove_member(actor, target, is_self, owners)?;
    state
        .store
        .check_not_sole_project_admin(ws, user_id)
        .await?;
    state.store.deactivate_ws_member(ws, user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn invites(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<Invite>>> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::Invite, facts(&ws.workspace))?;
    Ok(Json(state.store.pending_invites(ws.workspace.id).await?))
}

#[derive(Deserialize)]
pub struct InviteItem {
    email: String,
    role: WsRole,
}

#[derive(Deserialize)]
pub struct InviteReq {
    invites: Vec<InviteItem>,
}

#[derive(Serialize)]
pub struct InviteResult {
    email: String,
    role: WsRole,
    /// The link to share; shown once.
    #[serde(skip_serializing_if = "Option::is_none")]
    link: Option<String>,
    emailed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn invite(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(req): Json<InviteReq>,
) -> ApiResult<(StatusCode, Json<Vec<InviteResult>>)> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::Invite, facts(&ws.workspace))?;
    if req.invites.is_empty() || req.invites.len() > 50 {
        return Err(Error::bad_request("send between 1 and 50 invites at a time").into());
    }
    state.limiter.check(
        &format!("invite:{}", me.id),
        crate::ratelimit::INVITES_PER_USER,
    )?;
    let members = state.store.workspace_members(ws.workspace.id).await?;
    let base = state.base_url(&headers);
    let mut out = Vec::new();
    for item in req.invites {
        let fail = |email: String, msg: &str| InviteResult {
            email,
            role: item.role,
            link: None,
            emailed: false,
            error: Some(msg.into()),
        };
        let email = match user::normalize_email(&item.email) {
            Ok(e) => e,
            Err(e) => {
                out.push(fail(item.email, &e.message));
                continue;
            }
        };
        if !perm::can_invite_as(ws.role, item.role) {
            out.push(fail(email, "your role cannot invite with this role"));
            continue;
        }
        if members.iter().any(|m| m.email == email) {
            out.push(fail(email, "already a member"));
            continue;
        }
        let token = auth::random_token();
        state
            .store
            .create_invite(
                ws.workspace.id,
                &email,
                item.role,
                &auth::token_hash(&token),
                me.id,
                &auth::expires_in(INVITE_TTL),
            )
            .await?;
        let link = format!("{base}/invite/{token}");
        let emailed = if let Some(m) = &state.mailer {
            let who = if me.name.is_empty() {
                me.email.clone()
            } else {
                me.name.clone()
            };
            m.send_later(
                &email,
                format!("{who} invited you to {} on NextPlane", ws.workspace.name),
                format!(
                    "{who} invited you to join the workspace \"{}\" on NextPlane as {}.\n\nAccept the invite (valid 7 days):\n{link}\n",
                    ws.workspace.name,
                    item.role.as_str()
                ),
            );
            true
        } else {
            false
        };
        out.push(InviteResult {
            email,
            role: item.role,
            link: Some(link),
            emailed,
            error: None,
        });
    }
    Ok((StatusCode::CREATED, Json(out)))
}

pub async fn revoke(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::Invite, facts(&ws.workspace))?;
    state.store.revoke_invite(ws.workspace.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
