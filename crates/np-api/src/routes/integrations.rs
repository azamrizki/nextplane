//! Public boards, webhooks and GitHub (SPRINT S3.5).

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use np_core::id::Uuid;
use np_core::perm::ProjAction;
use np_core::{Error, User};
use np_store::integrations::{Delivery, PublicItem, Webhook};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::projects::load_project;
use super::workspaces::load;
use crate::AppState;
use crate::auth::{ClientIp, CurrentUser, random_token};
use crate::error::ApiResult;
use crate::events::Event;
use crate::extract::{Json, Path};
use crate::ratelimit::Limit;
use crate::webhooks::{AREAS, hex, hmac_sha256, same};

const BOARD_PER_IP: Limit = Limit {
    max: 240,
    window: std::time::Duration::from_secs(60),
};
const GITHUB_PER_LINK: Limit = Limit {
    max: 600,
    window: std::time::Duration::from_secs(60 * 60),
};

fn link_token() -> String {
    random_token().replace(['-', '_'], "x")
}

fn token_ok(t: &str) -> bool {
    !t.is_empty() && t.len() <= 128 && t.chars().all(|c| c.is_ascii_alphanumeric())
}

// ---- public boards -----------------------------------------------------------------------

#[derive(Serialize)]
pub struct Published {
    enabled: bool,
    path: Option<String>,
    /// What visitors who sign in may do (S3.7).
    comments: bool,
    reactions: bool,
    votes: bool,
}

async fn published(state: &AppState, pid: Uuid, token: Option<String>) -> ApiResult<Published> {
    let (comments, reactions, votes) = state.store.board_settings(pid).await?;
    Ok(Published {
        enabled: token.is_some(),
        path: token.map(|t| format!("/p/{t}")),
        comments,
        reactions,
        votes,
    })
}

pub async fn publish_settings(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
) -> ApiResult<Json<Published>> {
    let c = load_project(&state, &slug, pid, &me).await?;
    c.require(ProjAction::EditSettings)?;
    let t = state.store.public_board_token(pid).await?;
    Ok(Json(published(&state, pid, t).await?))
}

#[derive(Deserialize)]
pub struct EnableReq {
    enabled: Option<bool>,
    comments: Option<bool>,
    reactions: Option<bool>,
    votes: Option<bool>,
}

/// Publishing always makes a new link; the old one stops working. The settings for comments,
/// reactions and votes change without touching the link.
pub async fn set_publish(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Json(req): Json<EnableReq>,
) -> ApiResult<Json<Published>> {
    let c = load_project(&state, &slug, pid, &me).await?;
    c.require_write(ProjAction::EditSettings)?;
    state
        .store
        .set_board_settings(pid, req.comments, req.reactions, req.votes)
        .await?;
    let token = match req.enabled {
        Some(on) => {
            let token = on.then(link_token);
            state.store.set_public_board(pid, token.as_deref()).await?;
            token
        }
        None => state.store.public_board_token(pid).await?,
    };
    Ok(Json(published(&state, pid, token).await?))
}

#[derive(Serialize)]
pub struct Board {
    workspace: String,
    project: Value,
    /// What visitors who sign in may do.
    allow: Value,
    states: Vec<Value>,
    labels: Vec<Value>,
    items: Vec<PublicItem>,
}

async fn board_home(
    state: &AppState,
    ip: &str,
    token: &str,
) -> ApiResult<np_store::integrations::BoardHome> {
    state
        .limiter
        .check(&format!("board-ip:{ip}"), BOARD_PER_IP)?;
    if !token_ok(token) {
        return Err(Error::not_found("this board does not exist or is no longer public").into());
    }
    Ok(state
        .store
        .public_board(token)
        .await?
        .ok_or_else(|| Error::not_found("this board does not exist or is no longer public"))?)
}

pub async fn public_board(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Path(token): Path<String>,
) -> ApiResult<Json<Board>> {
    let (workspace, pid, name, identifier) = board_home(&state, &ip, &token).await?;
    let states = state
        .store
        .states(pid)
        .await?
        .into_iter()
        .map(|s| json!({ "id": s.id, "name": s.name, "color": s.color, "group": s.group }))
        .collect();
    let labels = state
        .store
        .labels(pid)
        .await?
        .into_iter()
        .map(|l| json!({ "id": l.id, "name": l.name, "color": l.color }))
        .collect();
    let items = state.store.public_items(pid, 2000).await?;
    let (comments, reactions, votes) = state.store.board_settings(pid).await?;
    Ok(Json(Board {
        workspace,
        project: json!({ "name": name, "identifier": identifier }),
        allow: json!({ "comments": comments, "reactions": reactions, "votes": votes }),
        states,
        labels,
        items,
    }))
}

pub async fn public_item(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Path((token, seq)): Path<(String, i64)>,
) -> ApiResult<Json<BoardItem>> {
    let b = board(&state, &ip, &token).await?;
    let me = crate::auth::peek_user_id(&state, &headers).await;
    Ok(Json(board_item_out(&state, &b, seq, me).await?))
}

// ---- visitors' comments, reactions and votes (S3.7) ---------------------------------------

const BOARD_WRITES: Limit = Limit {
    max: 60,
    window: std::time::Duration::from_secs(10 * 60),
};

#[derive(Serialize)]
pub struct Tally {
    emoji: String,
    count: usize,
    mine: bool,
}

#[derive(Serialize)]
pub struct Votes {
    up: usize,
    down: usize,
    /// Your vote: 1, -1 or 0.
    mine: i64,
}

#[derive(Serialize)]
pub struct BoardComment {
    id: Uuid,
    author_name: String,
    author_avatar: Option<String>,
    body_html: String,
    created_at: String,
    edited: bool,
    mine: bool,
    reactions: Vec<Tally>,
}

#[derive(Serialize)]
pub struct BoardItem {
    #[serde(flatten)]
    item: PublicItem,
    #[serde(skip_serializing_if = "Option::is_none")]
    reactions: Option<Vec<Tally>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    votes: Option<Votes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comments: Option<Vec<BoardComment>>,
}

fn tally(list: Vec<np_store::collab::ReactionCount>, me: Option<Uuid>) -> Vec<Tally> {
    list.into_iter()
        .map(|r| Tally {
            count: r.user_ids.len(),
            mine: me.is_some_and(|m| r.user_ids.contains(&m)),
            emoji: r.emoji,
        })
        .collect()
}

async fn board(state: &AppState, ip: &str, token: &str) -> ApiResult<np_store::board::BoardInfo> {
    board_home(state, ip, token).await?;
    Ok(state
        .store
        .board_info(token)
        .await?
        .ok_or_else(|| Error::not_found("this board does not exist or is no longer public"))?)
}

async fn board_item(state: &AppState, b: &np_store::board::BoardInfo, seq: i64) -> ApiResult<Uuid> {
    Ok(state
        .store
        .board_item_id(b.project_id, seq)
        .await?
        .ok_or_else(|| Error::not_found("work item not found"))?)
}

/// One item of a board with what visitors may see of comments, reactions and votes.
async fn board_item_out(
    state: &AppState,
    b: &np_store::board::BoardInfo,
    seq: i64,
    me: Option<Uuid>,
) -> ApiResult<BoardItem> {
    let item = state
        .store
        .public_item(b.project_id, seq)
        .await?
        .ok_or_else(|| Error::not_found("work item not found"))?;
    let wid = board_item(state, b, seq).await?;
    let reactions = if b.reactions {
        Some(tally(state.store.reactions("work_item", wid).await?, me))
    } else {
        None
    };
    let votes = if b.votes {
        let v = state.store.votes(wid).await?;
        Some(Votes {
            up: v.iter().filter(|x| x.1 > 0).count(),
            down: v.iter().filter(|x| x.1 < 0).count(),
            mine: v.iter().find(|x| Some(x.0) == me).map(|x| x.1).unwrap_or(0),
        })
    } else {
        None
    };
    let comments = if b.comments {
        let mut out = Vec::new();
        for c in state.store.public_comments(wid).await? {
            out.push(BoardComment {
                reactions: if b.reactions {
                    tally(state.store.reactions("comment", c.id).await?, me)
                } else {
                    Vec::new()
                },
                mine: Some(c.author_id) == me,
                edited: c.edited_at.is_some(),
                id: c.id,
                author_name: c.author_name,
                author_avatar: c.author_avatar,
                body_html: c.body_html,
                created_at: c.created_at,
            });
        }
        Some(out)
    } else {
        None
    };
    Ok(BoardItem {
        item,
        reactions,
        votes,
        comments,
    })
}

/// Open screens and webhooks hear about a visitor's comment, reaction or vote.
fn announce(state: &AppState, b: &np_store::board::BoardInfo, wid: Uuid, area: &str, action: &str, me: Uuid) {
    state.events.publish(Event {
        workspace: b.workspace_slug.clone(),
        workspace_id: Some(b.workspace_id),
        project_id: Some(b.project_id),
        work_item_id: Some(wid),
        area: area.into(),
        method: "POST".into(),
        action: action.into(),
        actor: Some(me),
        client: None,
    });
}

/// Signed in, the feature on, and not too fast.
async fn may_write(state: &AppState, b: &np_store::board::BoardInfo, me: &User, allowed: bool, what: &str) -> ApiResult<()> {
    if !allowed {
        return Err(Error::bad_request(format!("{what} are turned off on this board")).into());
    }
    if me.is_bot {
        return Err(Error::forbidden("bots cannot do this").into());
    }
    state
        .limiter
        .check(&format!("board-write:{}", me.id), BOARD_WRITES)?;
    let _ = b;
    Ok(())
}

pub async fn board_comment(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    CurrentUser(me): CurrentUser,
    Path((token, seq)): Path<(String, i64)>,
    Json(req): Json<super::collab::CommentReq>,
) -> ApiResult<(StatusCode, Json<BoardItem>)> {
    let b = board(&state, &ip, &token).await?;
    may_write(&state, &b, &me, b.comments, "comments").await?;
    let wid = board_item(&state, &b, seq).await?;
    let (json, html) = super::collab::clean_comment(req)?;
    let c = state.store.add_comment(wid, me.id, json.as_deref(), &html).await?;
    state.store.set_comment_public(c.id, true).await?;
    state.store.notify_comment(wid, me.id, c.id, &[]).await?;
    announce(&state, &b, wid, "comment", "created", me.id);
    Ok((StatusCode::CREATED, Json(board_item_out(&state, &b, seq, Some(me.id)).await?)))
}

/// Your own comment on a board: edit or delete.
async fn own_board_comment(
    state: &AppState,
    b: &np_store::board::BoardInfo,
    seq: i64,
    cid: Uuid,
    me: &User,
) -> ApiResult<Uuid> {
    let wid = board_item(state, b, seq).await?;
    let c = state
        .store
        .comment(wid, cid)
        .await?
        .filter(|c| c.is_public)
        .ok_or_else(|| Error::not_found("comment not found"))?;
    if c.author_id != me.id {
        return Err(Error::forbidden("only its author can change a comment").into());
    }
    Ok(wid)
}

pub async fn board_edit_comment(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    CurrentUser(me): CurrentUser,
    Path((token, seq, cid)): Path<(String, i64, Uuid)>,
    Json(req): Json<super::collab::CommentReq>,
) -> ApiResult<Json<BoardItem>> {
    let b = board(&state, &ip, &token).await?;
    may_write(&state, &b, &me, b.comments, "comments").await?;
    let wid = own_board_comment(&state, &b, seq, cid, &me).await?;
    let (json, html) = super::collab::clean_comment(req)?;
    state.store.edit_comment(cid, json.as_deref(), &html).await?;
    announce(&state, &b, wid, "comment", "updated", me.id);
    Ok(Json(board_item_out(&state, &b, seq, Some(me.id)).await?))
}

pub async fn board_delete_comment(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    CurrentUser(me): CurrentUser,
    Path((token, seq, cid)): Path<(String, i64, Uuid)>,
) -> ApiResult<Json<BoardItem>> {
    let b = board(&state, &ip, &token).await?;
    may_write(&state, &b, &me, b.comments, "comments").await?;
    let wid = own_board_comment(&state, &b, seq, cid, &me).await?;
    state.store.delete_comment(cid).await?;
    announce(&state, &b, wid, "comment", "deleted", me.id);
    Ok(Json(board_item_out(&state, &b, seq, Some(me.id)).await?))
}

#[derive(Deserialize)]
pub struct BoardReactReq {
    emoji: String,
    /// A comment of the item instead of the item itself.
    comment_id: Option<Uuid>,
}

pub async fn board_react(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    CurrentUser(me): CurrentUser,
    Path((token, seq)): Path<(String, i64)>,
    Json(req): Json<BoardReactReq>,
) -> ApiResult<Json<BoardItem>> {
    let b = board(&state, &ip, &token).await?;
    may_write(&state, &b, &me, b.reactions, "reactions").await?;
    let wid = board_item(&state, &b, seq).await?;
    let emoji = super::collab::clean_emoji(&req.emoji)?;
    match req.comment_id {
        Some(cid) => {
            state
                .store
                .comment(wid, cid)
                .await?
                .filter(|c| c.is_public && c.hidden_at.is_none())
                .ok_or_else(|| Error::not_found("comment not found"))?;
            state.store.toggle_reaction("comment", cid, me.id, &emoji).await?;
        }
        None => {
            state.store.toggle_reaction("work_item", wid, me.id, &emoji).await?;
        }
    }
    announce(&state, &b, wid, "reaction", "updated", me.id);
    Ok(Json(board_item_out(&state, &b, seq, Some(me.id)).await?))
}

#[derive(Deserialize)]
pub struct VoteReq {
    /// 1 (up), -1 (down) or 0 (take your vote back).
    value: i64,
}

pub async fn board_vote(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    CurrentUser(me): CurrentUser,
    Path((token, seq)): Path<(String, i64)>,
    Json(req): Json<VoteReq>,
) -> ApiResult<Json<BoardItem>> {
    let b = board(&state, &ip, &token).await?;
    may_write(&state, &b, &me, b.votes, "votes").await?;
    if !matches!(req.value, -1..=1) {
        return Err(Error::bad_request("a vote is 1, -1 or 0").into());
    }
    let wid = board_item(&state, &b, seq).await?;
    state.store.set_vote(wid, me.id, req.value).await?;
    announce(&state, &b, wid, "reaction", "updated", me.id);
    Ok(Json(board_item_out(&state, &b, seq, Some(me.id)).await?))
}

// ---- webhooks ----------------------------------------------------------------------------

async fn ws_admin(state: &AppState, slug: &str, me: &User) -> ApiResult<Uuid> {
    let ws = load(state, slug, me).await?;
    if !ws.role.is_admin() {
        return Err(Error::forbidden("only workspace admins manage integrations").into());
    }
    Ok(ws.workspace.id)
}

fn clean_url(u: &str) -> ApiResult<String> {
    let u = u.trim();
    let parsed =
        reqwest::Url::parse(u).map_err(|_| Error::bad_request("give a full http(s) URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() || u.len() > 2000
    {
        return Err(Error::bad_request("give a full http(s) URL").into());
    }
    Ok(u.to_string())
}

fn clean_events(ev: &[String]) -> ApiResult<String> {
    if ev.is_empty() || ev.iter().any(|e| e == "*") {
        return Ok("*".into());
    }
    for e in ev {
        if !AREAS.contains(&e.as_str()) {
            return Err(Error::bad_request(format!("unknown event {e:?}")).into());
        }
    }
    let mut v: Vec<&str> = ev.iter().map(String::as_str).collect();
    v.sort();
    v.dedup();
    Ok(v.join(","))
}

#[derive(Serialize)]
pub struct HookOut {
    #[serde(flatten)]
    hook: Webhook,
    /// Only when it is created or rotated: shown once.
    #[serde(skip_serializing_if = "Option::is_none")]
    secret: Option<String>,
}

pub async fn hooks(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<Webhook>>> {
    let ws = ws_admin(&state, &slug, &me).await?;
    Ok(Json(state.store.webhooks(ws).await?))
}

#[derive(Deserialize)]
pub struct HookReq {
    url: Option<String>,
    events: Option<Vec<String>>,
    active: Option<bool>,
}

pub async fn create_hook(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<HookReq>,
) -> ApiResult<(StatusCode, Json<HookOut>)> {
    let ws = ws_admin(&state, &slug, &me).await?;
    let url = clean_url(
        req.url
            .as_deref()
            .ok_or_else(|| Error::bad_request("give the URL"))?,
    )?;
    let events = clean_events(req.events.as_deref().unwrap_or(&[]))?;
    if state.store.webhooks(ws).await?.len() >= 20 {
        return Err(Error::bad_request("at most 20 webhooks per workspace").into());
    }
    let secret = format!("whsec_{}", link_token());
    let hook = state
        .store
        .create_webhook(ws, &url, &secret, &events, me.id)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(HookOut {
            hook,
            secret: Some(secret),
        }),
    ))
}

pub async fn update_hook(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
    Json(req): Json<HookReq>,
) -> ApiResult<Json<Webhook>> {
    let ws = ws_admin(&state, &slug, &me).await?;
    let url = req.url.as_deref().map(clean_url).transpose()?;
    let events = req.events.as_deref().map(clean_events).transpose()?;
    Ok(Json(
        state
            .store
            .update_webhook(ws, id, url.as_deref(), events.as_deref(), req.active, None)
            .await?,
    ))
}

pub async fn rotate_hook(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<HookOut>> {
    let ws = ws_admin(&state, &slug, &me).await?;
    let secret = format!("whsec_{}", link_token());
    let hook = state
        .store
        .update_webhook(ws, id, None, None, None, Some(&secret))
        .await?;
    Ok(Json(HookOut {
        hook,
        secret: Some(secret),
    }))
}

pub async fn delete_hook(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    let ws = ws_admin(&state, &slug, &me).await?;
    state.store.delete_webhook(ws, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn deliveries(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<Vec<Delivery>>> {
    let ws = ws_admin(&state, &slug, &me).await?;
    state
        .store
        .webhook(ws, id)
        .await?
        .ok_or_else(|| Error::not_found("webhook not found"))?;
    Ok(Json(state.store.deliveries(ws, id, 50).await?))
}

pub async fn redeliver(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id, did)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let ws = ws_admin(&state, &slug, &me).await?;
    state.store.redeliver(ws, id, did).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Queues a `ping` delivery.
pub async fn ping_hook(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    let ws = ws_admin(&state, &slug, &me).await?;
    state
        .store
        .webhook(ws, id)
        .await?
        .ok_or_else(|| Error::not_found("webhook not found"))?;
    let body =
        json!({ "event": "ping", "workspace": slug, "actor_id": me.id, "at": np_store::now_ts() });
    state
        .store
        .enqueue_delivery(id, "ping", &body.to_string())
        .await?;
    Ok(StatusCode::ACCEPTED)
}

// ---- GitHub ------------------------------------------------------------------------------

#[derive(Serialize)]
pub struct GithubOut {
    connected: bool,
    /// Where GitHub posts (`/api/v1/integrations/github/<token>`).
    path: Option<String>,
    /// Only right after connecting: shown once.
    #[serde(skip_serializing_if = "Option::is_none")]
    secret: Option<String>,
    close_on_merge: bool,
    last_event_at: Option<String>,
}

pub async fn github(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<GithubOut>> {
    let ws = ws_admin(&state, &slug, &me).await?;
    Ok(Json(match state.store.github(ws).await? {
        Some(g) => GithubOut {
            connected: true,
            path: Some(format!("/api/v1/integrations/github/{}", g.token)),
            secret: None,
            close_on_merge: g.close_on_merge,
            last_event_at: g.last_event_at,
        },
        None => GithubOut {
            connected: false,
            path: None,
            secret: None,
            close_on_merge: true,
            last_event_at: None,
        },
    }))
}

/// Connects, or makes a new link and secret.
pub async fn connect_github(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<Json<GithubOut>> {
    let ws = ws_admin(&state, &slug, &me).await?;
    let secret = link_token();
    let g = state
        .store
        .connect_github(ws, &link_token(), &secret, me.id)
        .await?;
    Ok(Json(GithubOut {
        connected: true,
        path: Some(format!("/api/v1/integrations/github/{}", g.token)),
        secret: Some(secret),
        close_on_merge: g.close_on_merge,
        last_event_at: g.last_event_at,
    }))
}

#[derive(Deserialize)]
pub struct GithubPatch {
    close_on_merge: bool,
}

pub async fn update_github(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<GithubPatch>,
) -> ApiResult<StatusCode> {
    let ws = ws_admin(&state, &slug, &me).await?;
    state
        .store
        .set_github_close_on_merge(ws, req.close_on_merge)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn disconnect_github(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
) -> ApiResult<StatusCode> {
    let ws = ws_admin(&state, &slug, &me).await?;
    state.store.disconnect_github(ws).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Work item keys (`WEB-12`, any case) of the given identifiers in a text, in order of
/// appearance, without repeats; `closing` marks keys right after "fixes", "closes" or
/// "resolves" (and their other forms).
pub fn find_keys(text: &str, identifiers: &[String]) -> Vec<(String, i64, bool)> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut out: Vec<(String, i64, bool)> = Vec::new();
    for ident in identifiers {
        let id = ident.to_ascii_lowercase();
        let mut from = 0;
        while let Some(pos) = lower[from..].find(&format!("{id}-")) {
            let start = from + pos;
            from = start + id.len() + 1;
            if start > 0 && bytes[start - 1].is_ascii_alphanumeric() {
                continue;
            }
            let digits: String = lower[from..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            let end = from + digits.len();
            if digits.is_empty()
                || digits.len() > 9
                || bytes.get(end).is_some_and(|b| b.is_ascii_alphabetic())
            {
                continue;
            }
            let Ok(seq) = digits.parse::<i64>() else {
                continue;
            };
            let before = lower[..start].trim_end_matches([' ', ':', '#', '(']);
            let closing = [
                "fix", "fixes", "fixed", "close", "closes", "closed", "resolve", "resolves",
                "resolved",
            ]
            .iter()
            .any(|w| {
                before.ends_with(w)
                    && before[..before.len() - w.len()]
                        .chars()
                        .last()
                        .is_none_or(|c| !c.is_ascii_alphanumeric())
            });
            match out.iter_mut().find(|(i, s, _)| i == ident && *s == seq) {
                Some(k) => k.2 |= closing,
                None => out.push((ident.clone(), seq, closing)),
            }
        }
    }
    out
}

#[derive(Serialize)]
pub struct GithubResult {
    linked: usize,
    closed: usize,
}

/// `POST /api/v1/integrations/github/{token}`: GitHub's webhook (content type JSON, events
/// "Pull requests" and "Pushes"). Pull requests and commits that mention `WEB-12` get a link
/// on the work item; a merged pull request that says "fixes WEB-12" completes it.
pub async fn github_hook(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<GithubResult>> {
    let not_found = || Error::not_found("unknown integration link");
    if !token_ok(&token) {
        return Err(not_found().into());
    }
    let g = state
        .store
        .github_by_token(&token)
        .await?
        .ok_or_else(not_found)?;
    state
        .limiter
        .check(&format!("github:{}", g.workspace_id), GITHUB_PER_LINK)?;
    let sig = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("sha256="))
        .unwrap_or("");
    let want = hex(&hmac_sha256(g.secret.as_bytes(), &body));
    if !same(sig.as_bytes(), want.as_bytes()) {
        return Err(Error::forbidden("bad signature").into());
    }
    state.store.github_seen(g.workspace_id).await?;
    let event = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let v: Value =
        serde_json::from_slice(&body).map_err(|_| Error::bad_request("the body must be JSON"))?;
    // (url, link title, text to search, merged)
    let mut refs: Vec<(String, String, String, bool)> = Vec::new();
    match event {
        "pull_request" => {
            let pr = &v["pull_request"];
            let n = pr["number"].as_i64().unwrap_or(0);
            let merged = pr["merged"].as_bool().unwrap_or(false);
            let st = if merged {
                "merged"
            } else if pr["state"] == "closed" {
                "closed"
            } else if pr["draft"].as_bool().unwrap_or(false) {
                "draft"
            } else {
                "open"
            };
            let title: String = pr["title"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(200)
                .collect();
            let text = format!(
                "{}\n{}\n{}",
                pr["title"].as_str().unwrap_or(""),
                pr["body"].as_str().unwrap_or(""),
                pr["head"]["ref"].as_str().unwrap_or("")
            );
            if let Some(url) = pr["html_url"].as_str() {
                refs.push((
                    url.to_string(),
                    format!("PR #{n}: {title} ({st})"),
                    text,
                    merged,
                ));
            }
        }
        "push" => {
            for c in v["commits"].as_array().into_iter().flatten().take(100) {
                let msg = c["message"].as_str().unwrap_or("");
                let first: String = msg.lines().next().unwrap_or("").chars().take(200).collect();
                let id: String = c["id"].as_str().unwrap_or("").chars().take(7).collect();
                if let Some(url) = c["url"].as_str() {
                    refs.push((
                        url.to_string(),
                        format!("Commit {id}: {first}"),
                        msg.to_string(),
                        false,
                    ));
                }
            }
        }
        _ => {
            return Ok(Json(GithubResult {
                linked: 0,
                closed: 0,
            }));
        }
    }
    let projects = state.store.project_identifiers(g.workspace_id).await?;
    let idents: Vec<String> = projects.iter().map(|(i, _)| i.clone()).collect();
    let slug = state
        .store
        .workspace_slug(g.workspace_id)
        .await?
        .unwrap_or_default();
    let mut out = GithubResult {
        linked: 0,
        closed: 0,
    };
    for (url, title, text, merged) in refs {
        if !url.starts_with("https://") {
            continue;
        }
        for (ident, seq, closing) in find_keys(&text, &idents) {
            let Some((pid, wid)) = state
                .store
                .work_item_by_key(g.workspace_id, &ident, seq)
                .await?
            else {
                continue;
            };
            state.store.upsert_link(wid, &url, &title, g.bot_id).await?;
            out.linked += 1;
            let mut action = "created";
            if merged && closing && g.close_on_merge {
                let done = state
                    .store
                    .states(pid)
                    .await?
                    .into_iter()
                    .filter(|s| s.group.0.as_str() == "completed")
                    .min_by(|a, b| {
                        (!a.is_default, a.sort_order)
                            .partial_cmp(&(!b.is_default, b.sort_order))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                let item = state.store.work_item(pid, wid).await?;
                if let (Some(done), Some(item)) = (done, item)
                    && item.state_id != done.id
                {
                    let patch = np_store::work_items::WorkItemPatch {
                        state_id: Some(done.id),
                        ..Default::default()
                    };
                    state
                        .store
                        .update_work_item(pid, wid, &patch, g.bot_id)
                        .await?;
                    out.closed += 1;
                    action = "updated";
                }
            }
            // Open screens refresh, and webhooks hear about it.
            state.events.publish(Event {
                workspace: slug.clone(),
                workspace_id: Some(g.workspace_id),
                project_id: Some(pid),
                work_item_id: Some(wid),
                area: if action == "updated" {
                    "work_item".into()
                } else {
                    "link".into()
                },
                method: "POST".into(),
                action: action.into(),
                actor: Some(g.bot_id),
                client: None,
            });
        }
    }
    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_in_text() {
        let ids = vec!["WEB".to_string(), "API".to_string()];
        let k = find_keys(
            "Fixes WEB-12 and relates to api-3; see WEB-12 again, not WEB-1x or XWEB-5 or WEB-",
            &ids,
        );
        assert_eq!(k, vec![("WEB".into(), 12, true), ("API".into(), 3, false)]);
        assert_eq!(
            find_keys("web-7-fix-login", &ids),
            vec![("WEB".into(), 7, false)]
        );
        assert_eq!(
            find_keys("closes: #WEB-9, prefix WEB-10", &ids),
            vec![("WEB".into(), 9, true), ("WEB".into(), 10, false)]
        );
        assert!(
            find_keys("prefixes WEB-4", &ids).iter().all(|k| !k.2),
            "\"prefixes\" is not \"fixes\""
        );
    }
}
