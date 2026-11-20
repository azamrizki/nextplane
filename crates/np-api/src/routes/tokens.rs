//! Personal API tokens, workspace bots (agents) and search.

use axum::extract::State;
use axum::http::StatusCode;
use np_core::id::Uuid;
use np_core::perm::{self, ProjAction, WsAction, WsRole, ws_require};
use np_core::user::{self, NAME_MAX};
use np_core::{Error, User};
use np_store::search::{ApiToken, SearchHit};
use serde::{Deserialize, Serialize};
use time::Duration;

use super::workspaces::{facts, load};
use crate::AppState;
use crate::auth::{self, CurrentUser, TOKEN_PREFIX};
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};

#[derive(Deserialize)]
pub struct TokenReq {
    name: String,
    #[serde(default = "default_scope")]
    scope: String,
    expires_in_days: Option<i64>,
}

fn default_scope() -> String {
    "write".into()
}

#[derive(Serialize)]
pub struct NewToken {
    #[serde(flatten)]
    meta: ApiToken,
    /// Shown once.
    token: String,
}

fn clean_token_req(req: &TokenReq) -> ApiResult<(String, Option<String>)> {
    let name = user::normalize_name(&req.name, NAME_MAX, "token name")?;
    if name.is_empty() {
        return Err(Error::bad_request("give the token a name").into());
    }
    if req.scope != "read" && req.scope != "write" {
        return Err(Error::bad_request("scope is read or write").into());
    }
    let exp = match req.expires_in_days {
        None => None,
        Some(d) if (1..=3650).contains(&d) => Some(auth::expires_in(Duration::days(d))),
        Some(_) => return Err(Error::bad_request("expiry is 1 to 3650 days").into()),
    };
    Ok((name, exp))
}

fn new_secret() -> String {
    format!("{TOKEN_PREFIX}{}", auth::random_token())
}

pub async fn list(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<Vec<ApiToken>>> {
    Ok(Json(state.store.tokens(me.id).await?))
}

pub async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(req): Json<TokenReq>,
) -> ApiResult<(StatusCode, Json<NewToken>)> {
    if me.is_bot {
        return Err(Error::forbidden("bots cannot make tokens").into());
    }
    let (name, exp) = clean_token_req(&req)?;
    let token = new_secret();
    let meta = state
        .store
        .create_token(
            me.id,
            &name,
            &auth::token_hash(&token),
            &req.scope,
            exp.as_deref(),
            me.id,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(NewToken { meta, token })))
}

pub async fn revoke(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state.store.revoke_token(me.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- bots -------------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Bot {
    #[serde(flatten)]
    user: User,
    tokens: Vec<ApiToken>,
}

pub async fn bots(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<Bot>>> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::Invite, facts(&ws.workspace))?;
    let mut out = Vec::new();
    for user in state.store.workspace_bots(ws.workspace.id).await? {
        let tokens = state.store.tokens(user.id).await?;
        out.push(Bot { user, tokens });
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct BotReq {
    name: String,
    #[serde(default = "member")]
    role: WsRole,
    #[serde(default = "default_scope")]
    scope: String,
}

fn member() -> WsRole {
    WsRole::Member
}

/// A bot is a workspace member without a password that works through its token, e.g. a
/// NextBoard agent. Its activity shows under the bot's name.
pub async fn create_bot(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<BotReq>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::Invite, facts(&ws.workspace))?;
    if !perm::can_invite_as(ws.role, req.role) {
        return Err(Error::forbidden("your role cannot give a bot this role").into());
    }
    let treq = TokenReq {
        name: req.name.clone(),
        scope: req.scope.clone(),
        expires_in_days: None,
    };
    let (name, _) = clean_token_req(&treq)?;
    let bot = state
        .store
        .create_bot(ws.workspace.id, &name, req.role)
        .await?;
    let token = new_secret();
    state
        .store
        .create_token(
            bot.id,
            &name,
            &auth::token_hash(&token),
            &req.scope,
            None,
            me.id,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "bot": bot, "token": token })),
    ))
}

pub async fn remove_bot(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::Invite, facts(&ws.workspace))?;
    state.store.remove_bot(ws.workspace.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A new token for a bot (the old ones keep working until revoked).
pub async fn rotate_bot_token(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let ws = load(&state, &slug, &me).await?;
    ws_require(ws.role, WsAction::Invite, facts(&ws.workspace))?;
    if state.store.bot_workspace(id).await? != Some(ws.workspace.id) {
        return Err(Error::not_found("bot not found").into());
    }
    let bot = state
        .store
        .user(id)
        .await?
        .ok_or_else(|| Error::not_found("bot not found"))?;
    for t in state.store.tokens(id).await? {
        state.store.revoke_token(id, t.id).await?;
    }
    let token = new_secret();
    state
        .store
        .create_token(
            id,
            &bot.name,
            &auth::token_hash(&token),
            "write",
            None,
            me.id,
        )
        .await?;
    Ok(Json(serde_json::json!({ "token": token })))
}

// ---- search -----------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    limit: Option<i64>,
}

#[derive(Serialize)]
pub struct SearchResult {
    projects: Vec<serde_json::Value>,
    work_items: Vec<SearchHit>,
    pages: Vec<serde_json::Value>,
}

/// Searches work item titles, descriptions and comments in every project the caller may
/// read, plus project names, and resolves keys like `WEB-12`.
pub async fn search(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<SearchResult>> {
    let ws = load(&state, &slug, &me).await?;
    let limit = q.limit.unwrap_or(20).clamp(1, 50);
    let rows = state
        .store
        .workspace_projects(ws.workspace.id, me.id, false)
        .await?;
    let mut readable = Vec::new();
    let mut projects = Vec::new();
    let needle = q.q.trim().to_lowercase();
    for r in &rows {
        if !perm::can_see_project(ws.role, r.member_role.0, r.project.is_private) {
            continue;
        }
        let role = perm::effective_proj_role(ws.role, r.member_role.0);
        if role.is_some_and(|r| perm::proj_can(r, ProjAction::ViewWorkItems)) {
            readable.push(r.project.id);
        }
        if !needle.is_empty()
            && (r.project.name.to_lowercase().contains(&needle)
                || r.project.identifier.to_lowercase() == needle)
        {
            projects.push(serde_json::json!({ "id": r.project.id, "name": r.project.name, "identifier": r.project.identifier, "icon": r.project.icon }));
        }
    }
    let mut work_items = state.store.search(&readable, &q.q, limit).await?;
    // An exact key like WEB-12 comes first.
    if let Some((ident, seq)) =
        q.q.trim()
            .rsplit_once('-')
            .and_then(|(i, s)| s.parse::<i64>().ok().map(|s| (i.to_uppercase(), s)))
        && let Some((pid, id)) = state
            .store
            .work_item_by_key(ws.workspace.id, &ident, seq)
            .await?
        && readable.contains(&pid)
        && !work_items.iter().any(|h| h.work_item_id == id)
        && let Some(w) = state.store.work_item(pid, id).await?
    {
        work_items.insert(
            0,
            SearchHit {
                kind: "key".into(),
                work_item_id: id,
                project_id: pid,
                identifier: ident,
                sequence: seq,
                title: w.title,
                snippet: String::new(),
                state_id: w.state_id,
            },
        );
    }
    let pages = state
        .store
        .search_pages(&readable, &q.q, 10)
        .await?
        .into_iter()
        .map(|(id, project_id, title, snippet)| serde_json::json!({ "id": id, "project_id": project_id, "title": title, "snippet": snippet }))
        .collect();
    Ok(Json(SearchResult {
        projects,
        work_items,
        pages,
    }))
}
