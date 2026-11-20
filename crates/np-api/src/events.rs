//! Realtime: every successful change under `/api/v1/workspaces/{slug}/…` becomes an event on
//! an in-process bus, and `/api/v1/ws` forwards events to subscribed browsers that may see
//! them. Events say *what area changed*; clients refetch what they show (SPEC §8).

use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use np_core::id::Uuid;
use np_core::perm;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::AppState;
use crate::auth::CurrentUser;

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub workspace: String,
    #[serde(skip)]
    pub workspace_id: Option<Uuid>,
    /// Set for changes inside one project.
    pub project_id: Option<Uuid>,
    /// Set for changes to one work item (and its comments, files…).
    pub work_item_id: Option<Uuid>,
    /// `work_item`, `comment`, `project`, `member`, `workspace`, …
    pub area: String,
    pub method: String,
    /// `created`, `updated`, `deleted`, or what a POST did (`archived`, `accepted`, `lock`…).
    pub action: String,
    pub actor: Option<Uuid>,
    /// The `X-Client-Id` of the tab that made the change, so it can skip its own echo.
    pub client: Option<String>,
}

#[derive(Clone)]
pub struct Bus {
    tx: broadcast::Sender<Event>,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            tx: broadcast::channel(1024).0,
        }
    }
}

impl Bus {
    pub fn publish(&self, e: Event) {
        let _ = self.tx.send(e);
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

/// Reads `/api/v1/workspaces/{slug}/[projects/{id}/[work-items/{wid}/]]{area}`.
pub fn parse_path(path: &str) -> Option<(String, Option<Uuid>, Option<Uuid>, String)> {
    let rest = path.strip_prefix("/api/v1/workspaces/")?;
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    let slug = (*parts.first()?).to_string();
    if slug == "trash" || slug == "slug-check" {
        return None;
    }
    let mut project = None;
    let mut item = None;
    let mut area = "workspace".to_string();
    let mut i = 1;
    while i < parts.len() {
        match parts[i] {
            "projects" => {
                area = "project".into();
                project = parts.get(i + 1).and_then(|p| p.parse().ok());
                i += 2;
            }
            "work-items" => {
                area = "work_item".into();
                item = parts.get(i + 1).and_then(|p| p.parse().ok());
                i += 2;
            }
            other => {
                area = match other {
                    "comments" => "comment",
                    "attachments" => "attachment",
                    "links" if area == "module" => "module",
                    "links" => "link",
                    "reactions" => "reaction",
                    "relations" => "relation",
                    "subscription" => "subscription",
                    "members" | "leave" | "join" => "member",
                    "states" => "state",
                    "labels" => "label",
                    "invites" => "invite",
                    "cycles" | "transfer" | "progress" => "cycle",
                    "modules" => "module",
                    "views" => "view",
                    "pages" | "do" => "page",
                    "intake" | "intake-form" => "intake",
                    "import" => "work_item",
                    "publish" => "project",
                    "webhooks" => "webhook",
                    "github" => "integration",
                    "notifications" | "read-all" => "subscription",
                    _ => area.as_str(),
                }
                .to_string();
                i += 1;
            }
        }
    }
    Some((slug, project, item, area))
}

/// Path segments that name a collection: a POST to one creates something.
const COLLECTIONS: &[&str] = &[
    "projects",
    "work-items",
    "comments",
    "attachments",
    "links",
    "reactions",
    "relations",
    "members",
    "states",
    "labels",
    "invites",
    "cycles",
    "modules",
    "views",
    "pages",
    "intake",
    "webhooks",
    "uploads",
    "import",
    "versions",
    "join",
];

/// What a successful request did, for webhook event names (`work_item.created`).
pub fn action_of(method: &Method, path: &str) -> String {
    let last = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    match *method {
        Method::PATCH | Method::PUT => "updated".into(),
        Method::DELETE => "deleted".into(),
        _ if COLLECTIONS.contains(&last) || last.parse::<Uuid>().is_ok() => "created".into(),
        _ => match last {
            "archive" => "archived".into(),
            "unarchive" => "unarchived".into(),
            "accept" => "accepted".into(),
            "decline" => "declined".into(),
            "snooze" => "snoozed".into(),
            "reopen" => "reopened".into(),
            "publish" => "published".into(),
            "transfer" => "transferred".into(),
            other => other.replace('-', "_"),
        },
    }
}

/// Publishes an event after every successful state-changing request in a workspace.
pub async fn publish_changes(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let client = req
        .headers()
        .get("x-client-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(64).collect());
    let changes = !matches!(method, Method::GET | Method::HEAD | Method::OPTIONS);
    let actor = if changes {
        crate::auth::peek_user_id(&state, req.headers()).await
    } else {
        None
    };
    let mut res = next.run(req).await;
    if changes
        && res.status().is_success()
        && let Some((workspace, project_id, mut work_item_id, area)) = parse_path(&path)
    {
        // A new work item: its id is only in the answer (a small JSON object).
        if method == Method::POST
            && work_item_id.is_none()
            && (path.ends_with("/work-items") || path.ends_with("/intake"))
        {
            let (parts, body) = res.into_parts();
            let bytes = axum::body::to_bytes(body, 1024 * 1024)
                .await
                .unwrap_or_default();
            work_item_id = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|v| {
                    v.get("id")
                        .or_else(|| v.get("work_item_id"))
                        .and_then(|i| i.as_str())
                        .and_then(|i| i.parse().ok())
                });
            res = Response::from_parts(parts, axum::body::Body::from(bytes));
        }
        // Rights may have changed: open page sockets reconnect and are checked again.
        let last = path.rsplit('/').next().unwrap_or("");
        match area.as_str() {
            "member" => state.rooms.kick_project(project_id),
            "project"
                if last.parse::<Uuid>().is_ok() || last == "archive" || last == "unarchive" =>
            {
                state.rooms.kick_project(project_id)
            }
            _ => {}
        }
        if area != "subscription" {
            let action = action_of(&method, &path);
            state.events.publish(Event {
                workspace,
                workspace_id: None,
                project_id,
                work_item_id,
                area,
                method: method.to_string(),
                action,
                actor,
                client,
            });
        }
    }
    res
}

#[derive(Deserialize)]
struct ClientMsg {
    subscribe: Option<String>,
}

/// `GET /api/v1/ws`: send `{"subscribe": "<workspace slug>"}`; receive events as JSON.
pub async fn ws(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    headers: axum::http::HeaderMap,
    up: WebSocketUpgrade,
) -> Response {
    if !crate::origin::websocket_allowed(&headers) {
        return crate::error::ApiError(np_core::Error::forbidden("cross-origin socket refused"))
            .into_response();
    }
    up.on_upgrade(move |socket| serve(state, me.id, socket))
}

async fn serve(state: AppState, user: Uuid, mut socket: WebSocket) {
    let mut rx = state.events.subscribe();
    let mut slug: Option<(String, Uuid)> = None;
    let mut ping = tokio::time::interval(Duration::from_secs(25));
    loop {
        tokio::select! {
            msg = socket.recv() => {
                let Some(Ok(msg)) = msg else { break };
                if let Message::Text(t) = msg
                    && let Ok(ClientMsg { subscribe: Some(s) }) = serde_json::from_str::<ClientMsg>(&t) {
                        // Only workspaces the user is an active member of.
                        slug = match state.store.workspace_for(&s, user).await {
                            Ok(w) => Some((w.workspace.slug, w.workspace.id)),
                            Err(_) => None,
                        };
                        let ack = serde_json::json!({ "subscribed": slug.as_ref().map(|s| s.0.clone()) });
                        if socket.send(Message::Text(ack.to_string().into())).await.is_err() { break; }
                    }
            }
            ev = rx.recv() => {
                let ev = match ev {
                    Ok(ev) => ev,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Too far behind: tell the client to refetch everything.
                        if socket.send(Message::Text(r#"{"area":"resync"}"#.into())).await.is_err() { break; }
                        continue;
                    }
                    Err(_) => break,
                };
                let Some((s, ws_id)) = &slug else { continue };
                if &ev.workspace != s || !may_see(&state, user, *ws_id, ev.project_id).await { continue; }
                let Ok(text) = serde_json::to_string(&ev) else { continue };
                if socket.send(Message::Text(text.into())).await.is_err() { break; }
            }
            _ = ping.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
            }
        }
    }
}

/// Whether the user still belongs to the workspace and may see the project of an event.
async fn may_see(state: &AppState, user: Uuid, ws: Uuid, project: Option<Uuid>) -> bool {
    let Ok(Some(role)) = state.store.ws_role(ws, user).await else {
        return false;
    };
    let Some(pid) = project else { return true };
    let explicit = state.store.proj_role(pid, user).await.ok().flatten();
    if role.is_admin() || explicit.is_some() {
        return true;
    }
    // A public project the member has not joined: they see it listed, so list changes matter.
    let private: Option<bool> = sqlx_is_private(state, pid).await;
    private == Some(false) && perm::can_see_project(role, None, false)
}

async fn sqlx_is_private(state: &AppState, pid: Uuid) -> Option<bool> {
    state.store.project_is_private(pid).await.ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::parse_path;

    #[test]
    fn paths() {
        let p = "0190a4e8-0000-7000-8000-000000000001";
        let w = "0190a4e8-0000-7000-8000-000000000002";
        assert_eq!(
            parse_path("/api/v1/workspaces/acme").unwrap().3,
            "workspace"
        );
        assert_eq!(
            parse_path("/api/v1/workspaces/acme/members/x").unwrap().3,
            "member"
        );
        let (s, pid, wid, area) = parse_path(&format!(
            "/api/v1/workspaces/acme/projects/{p}/work-items/{w}/comments/c1"
        ))
        .unwrap();
        assert_eq!((s.as_str(), area.as_str()), ("acme", "comment"));
        assert_eq!(pid.unwrap().to_string(), p);
        assert_eq!(wid.unwrap().to_string(), w);
        assert_eq!(
            parse_path(&format!("/api/v1/workspaces/acme/projects/{p}/states/s"))
                .unwrap()
                .3,
            "state"
        );
        assert!(parse_path("/api/v1/me").is_none());
        assert!(parse_path("/api/v1/workspaces/trash").is_none());
    }
}
