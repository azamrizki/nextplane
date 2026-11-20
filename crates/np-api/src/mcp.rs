//! MCP server (Model Context Protocol, streamable HTTP, JSON responses) at `POST /mcp`.
//!
//! Agents authenticate with an API token (`Authorization: Bearer np_…`), usually a bot's.
//! Every tool calls the REST API internally with the caller's credentials, so an agent can do
//! exactly what its account may do — the permission rules live in one place.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::AppState;

const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

pub async fn no_stream() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        "this MCP server answers POST requests with JSON; it has no event stream",
    )
        .into_response()
}

pub async fn handle(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return Json(rpc_error(Value::Null, -32700, &format!("parse error: {e}")))
                .into_response();
        }
    };
    let call = Caller {
        state: &state,
        headers: &headers,
    };
    if let Value::Array(batch) = msg {
        let mut out = Vec::new();
        for m in batch {
            if let Some(r) = call.one(m).await {
                out.push(r);
            }
        }
        return if out.is_empty() {
            StatusCode::ACCEPTED.into_response()
        } else {
            Json(Value::Array(out)).into_response()
        };
    }
    match call.one(msg).await {
        Some(r) => Json(r).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

use axum::Json;

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

struct Caller<'a> {
    state: &'a AppState,
    headers: &'a HeaderMap,
}

/// A tool failed with a message for the agent.
struct ToolErr(String);

impl From<String> for ToolErr {
    fn from(s: String) -> Self {
        ToolErr(s)
    }
}
impl From<&str> for ToolErr {
    fn from(s: &str) -> Self {
        ToolErr(s.into())
    }
}

type ToolResult = Result<Value, ToolErr>;

impl Caller<'_> {
    /// Handles one JSON-RPC message; `None` for notifications (no id).
    async fn one(&self, m: Value) -> Option<Value> {
        let id = m.get("id").cloned();
        let method = m.get("method").and_then(Value::as_str).unwrap_or("");
        let params = m.get("params").cloned().unwrap_or(Value::Null);
        let id = id?; // notifications get no answer
        Some(match method {
            "initialize" => {
                let asked = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or(PROTOCOLS[0]);
                let version = if PROTOCOLS.contains(&asked) {
                    asked
                } else {
                    PROTOCOLS[0]
                };
                ok(
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": { "tools": { "listChanged": false } },
                        "serverInfo": { "name": "nextplane", "version": self.state.version.version },
                        "instructions": "NextPlane project management. Work items have keys like WEB-12 (project identifier and number). \
                            Start with list_workspaces and list_projects. States, priorities and people are given by name; \
                            'me' means the account behind your token."
                    }),
                )
            }
            "ping" => ok(id, json!({})),
            "tools/list" => ok(id, json!({ "tools": tools() })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match self.tool(name, &args).await {
                    Ok(v) => ok(
                        id,
                        json!({ "content": [{ "type": "text", "text": text_of(&v) }], "structuredContent": wrap(v), "isError": false }),
                    ),
                    Err(ToolErr(e)) => ok(
                        id,
                        json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
                    ),
                }
            }
            _ => rpc_error(id, -32601, &format!("method not found: {method}")),
        })
    }

    /// Calls the REST API as the MCP caller. Errors carry the API's message.
    async fn api(&self, method: Method, path: &str, body: Option<Value>) -> ToolResult {
        let Some(router) = self.state.inner.get() else {
            return Err("server not ready".into());
        };
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"));
        for h in [header::AUTHORIZATION, header::COOKIE] {
            if let Some(v) = self.headers.get(&h) {
                req = req.header(h, v);
            }
        }
        if let Some(v) = self.headers.get("x-forwarded-for") {
            req = req.header("x-forwarded-for", v);
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .map_err(|e| e.to_string())?;
        let res = router
            .clone()
            .oneshot(req)
            .await
            .map_err(|e| e.to_string())?;
        let status = res.status();
        let bytes = res
            .into_body()
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .to_bytes();
        let v: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        if status.is_success() {
            Ok(v)
        } else {
            let msg = v["error"]["message"].as_str().unwrap_or("request failed");
            Err(format!("{} ({})", msg, status.as_u16()).into())
        }
    }

    async fn project(&self, ws: &str, p: &str) -> ToolResult {
        let list = self
            .api(Method::GET, &format!("/workspaces/{ws}/projects"), None)
            .await?;
        list.as_array()
            .and_then(|a| {
                a.iter().find(|x| {
                    x["identifier"]
                        .as_str()
                        .is_some_and(|i| i.eq_ignore_ascii_case(p))
                        || x["id"] == p
                        || x["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case(p))
                })
            })
            .cloned()
            .ok_or_else(|| format!("no project {p:?} in {ws} that you can see").into())
    }

    /// Resolves `WEB-12` to (project id, item id, project identifier).
    async fn key(&self, ws: &str, key: &str) -> Result<(String, String, String), ToolErr> {
        let r = self
            .api(
                Method::GET,
                &format!("/workspaces/{ws}/work-items/by-key/{}", urlenc(key)),
                None,
            )
            .await?;
        let ident = key
            .rsplit_once('-')
            .map(|x| x.0.to_uppercase())
            .unwrap_or_default();
        Ok((s(&r["project_id"]), s(&r["id"]), ident))
    }

    async fn lookups(&self, ws: &str, pid: &str) -> Result<Lookups, ToolErr> {
        let base = format!("/workspaces/{ws}/projects/{pid}");
        let states = self
            .api(Method::GET, &format!("{base}/states"), None)
            .await?;
        let members = self
            .api(Method::GET, &format!("{base}/members"), None)
            .await?;
        let labels = self
            .api(Method::GET, &format!("{base}/labels"), None)
            .await?;
        let me = self.api(Method::GET, "/me", None).await?;
        Ok(Lookups {
            states,
            members,
            labels,
            me: s(&me["id"]),
        })
    }

    async fn tool(&self, name: &str, a: &Value) -> ToolResult {
        let ws = || {
            a["workspace"]
                .as_str()
                .ok_or(ToolErr("give \"workspace\" (its slug)".into()))
        };
        match name {
            "list_workspaces" => {
                let v = self.api(Method::GET, "/workspaces", None).await?;
                Ok(Value::Array(
                    v.as_array()
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .map(|w| json!({ "slug": w["slug"], "name": w["name"], "role": w["role"] }))
                        .collect(),
                ))
            }
            "list_projects" => {
                let v = self
                    .api(
                        Method::GET,
                        &format!("/workspaces/{}/projects", ws()?),
                        None,
                    )
                    .await?;
                Ok(Value::Array(
                    v.as_array().cloned().unwrap_or_default().iter()
                        .map(|p| json!({ "identifier": p["identifier"], "name": p["name"], "my_role": p["my_role"], "joined": p["is_member"], "description": p["description"] }))
                        .collect(),
                ))
            }
            "list_states" | "list_members" | "list_labels" | "list_cycles" => {
                let w = ws()?;
                let p = self.project(w, a["project"].as_str().unwrap_or("")).await?;
                let what = name.trim_start_matches("list_");
                let v = self
                    .api(
                        Method::GET,
                        &format!("/workspaces/{w}/projects/{}/{what}", s(&p["id"])),
                        None,
                    )
                    .await?;
                let rows = v.as_array().cloned().unwrap_or_default();
                Ok(Value::Array(rows.iter().map(|x| match what {
                    "states" => json!({ "name": x["name"], "group": x["group"], "default": x["is_default"] }),
                    "members" => json!({ "name": x["name"], "email": x["email"], "role": x["role"] }),
                    "labels" => json!({ "name": x["name"] }),
                    _ => json!({ "name": x["name"], "status": x["status"], "start_date": x["start_date"], "end_date": x["end_date"], "done": x["completed_items"], "total": x["total_items"] }),
                }).collect()))
            }
            "list_work_items" => {
                let w = ws()?;
                let p = self.project(w, a["project"].as_str().unwrap_or("")).await?;
                let pid = s(&p["id"]);
                let lk = self.lookups(w, &pid).await?;
                let mut q = vec![format!(
                    "limit={}",
                    a["limit"].as_i64().unwrap_or(50).clamp(1, 200)
                )];
                if let Some(g) = a["state_group"].as_str() {
                    q.push(format!("state_group={}", urlenc(g)));
                }
                if let Some(st) = a["state"].as_str() {
                    q.push(format!("state={}", lk.state_id(st)?));
                }
                if let Some(pr) = a["priority"].as_str() {
                    q.push(format!("priority={}", urlenc(pr)));
                }
                if let Some(who) = a["assignee"].as_str() {
                    q.push(format!("assignee={}", lk.user_id(who)?));
                }
                if let Some(t) = a["query"].as_str() {
                    q.push(format!("q={}", urlenc(t)));
                }
                let v = self
                    .api(
                        Method::GET,
                        &format!("/workspaces/{w}/projects/{pid}/work-items?{}", q.join("&")),
                        None,
                    )
                    .await?;
                let ident = s(&p["identifier"]);
                Ok(Value::Array(
                    v.as_array()
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .map(|x| lk.summary(x, &ident))
                        .collect(),
                ))
            }
            "get_work_item" => {
                let w = ws()?;
                let (pid, id, ident) = self.key(w, a["key"].as_str().unwrap_or("")).await?;
                let lk = self.lookups(w, &pid).await?;
                let base = format!("/workspaces/{w}/projects/{pid}/work-items/{id}");
                let item = self.api(Method::GET, &base, None).await?;
                let comments = self
                    .api(Method::GET, &format!("{base}/comments"), None)
                    .await?;
                let mut out = lk.summary(&item, &ident);
                out["description"] = json!(np_store::search::html_text(
                    item["description_html"].as_str().unwrap_or("")
                ));
                out["start_date"] = item["start_date"].clone();
                out["estimate"] = item["estimate"].clone();
                out["comments"] = Value::Array(comments.as_array().cloned().unwrap_or_default().iter().map(|c| json!({
                    "author": lk.user_name(c["author_id"].as_str().unwrap_or("")),
                    "text": np_store::search::html_text(c["body_html"].as_str().unwrap_or("")),
                    "at": c["created_at"],
                })).collect());
                Ok(out)
            }
            "create_work_item" => {
                let w = ws()?;
                let p = self.project(w, a["project"].as_str().unwrap_or("")).await?;
                let pid = s(&p["id"]);
                let lk = self.lookups(w, &pid).await?;
                let mut body = json!({ "title": a["title"].as_str().ok_or("give a \"title\"")? });
                lk.apply(a, &mut body, self, w).await?;
                let v = self
                    .api(
                        Method::POST,
                        &format!("/workspaces/{w}/projects/{pid}/work-items"),
                        Some(body),
                    )
                    .await?;
                Ok(lk.summary(&v, &s(&p["identifier"])))
            }
            "update_work_item" => {
                let w = ws()?;
                let (pid, id, ident) = self.key(w, a["key"].as_str().unwrap_or("")).await?;
                let lk = self.lookups(w, &pid).await?;
                let mut body = json!({});
                if let Some(t) = a["title"].as_str() {
                    body["title"] = json!(t);
                }
                lk.apply(a, &mut body, self, w).await?;
                if body.as_object().is_some_and(|o| o.is_empty()) {
                    return Err("nothing to change".into());
                }
                let v = self
                    .api(
                        Method::PATCH,
                        &format!("/workspaces/{w}/projects/{pid}/work-items/{id}"),
                        Some(body),
                    )
                    .await?;
                Ok(lk.summary(&v, &ident))
            }
            "add_comment" => {
                let w = ws()?;
                let (pid, id, ident) = self.key(w, a["key"].as_str().unwrap_or("")).await?;
                let text = a["text"].as_str().ok_or("give the comment \"text\"")?;
                self.api(
                    Method::POST,
                    &format!("/workspaces/{w}/projects/{pid}/work-items/{id}/comments"),
                    Some(json!({ "body_html": text_to_html(text) })),
                )
                .await?;
                Ok(json!({ "commented_on": a["key"], "project": ident }))
            }
            "search" => {
                let w = ws()?;
                let q = a["query"].as_str().ok_or("give a \"query\"")?;
                let v = self
                    .api(
                        Method::GET,
                        &format!("/workspaces/{w}/search?q={}&limit=20", urlenc(q)),
                        None,
                    )
                    .await?;
                let mut hits: Vec<Value> = v["work_items"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|h| {
                        json!({
                            "key": format!("{}-{}", s(&h["identifier"]), h["sequence"]),
                            "title": h["title"],
                            "match": h["snippet"],
                        })
                    })
                    .collect();
                hits.extend(
                    v["pages"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .map(|h| {
                            json!({
                                "page": h["id"],
                                "project": h["project_id"],
                                "title": h["title"],
                                "match": h["snippet"],
                            })
                        }),
                );
                Ok(Value::Array(hits))
            }
            "list_pages" => {
                let w = ws()?;
                let p = self.project(w, a["project"].as_str().unwrap_or("")).await?;
                let v = self
                    .api(
                        Method::GET,
                        &format!("/workspaces/{w}/projects/{}/pages", s(&p["id"])),
                        None,
                    )
                    .await?;
                Ok(Value::Array(
                    v.as_array()
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .map(|x| {
                            json!({
                                "page": x["id"],
                                "title": x["title"],
                                "parent": x["parent_id"],
                                "private": x["access"] == "private",
                                "locked": !x["locked_at"].is_null(),
                                "updated_at": x["updated_at"],
                            })
                        })
                        .collect(),
                ))
            }
            "get_page" => {
                let w = ws()?;
                let p = self.project(w, a["project"].as_str().unwrap_or("")).await?;
                let id = a["page"]
                    .as_str()
                    .ok_or("give \"page\" (its id from list_pages or search)")?;
                let x = self
                    .api(
                        Method::GET,
                        &format!(
                            "/workspaces/{w}/projects/{}/pages/{}",
                            s(&p["id"]),
                            urlenc(id)
                        ),
                        None,
                    )
                    .await?;
                Ok(json!({
                    "page": x["id"],
                    "title": x["title"],
                    "text": np_store::search::html_text(x["html"].as_str().unwrap_or("")),
                    "locked": !x["locked_at"].is_null(),
                    "updated_at": x["updated_at"],
                }))
            }
            "create_page" => {
                let w = ws()?;
                let p = self.project(w, a["project"].as_str().unwrap_or("")).await?;
                let base = format!("/workspaces/{w}/projects/{}/pages", s(&p["id"]));
                let mut body = json!({ "title": a["title"].as_str().ok_or("give a \"title\"")? });
                if let Some(parent) = a["parent"].as_str() {
                    body["parent_id"] = json!(parent);
                }
                let v = self.api(Method::POST, &base, Some(body)).await?;
                if let Some(text) = a["text"].as_str().filter(|t| !t.trim().is_empty()) {
                    self.api(
                        Method::PATCH,
                        &format!("{base}/{}", s(&v["id"])),
                        Some(json!({ "html": text_to_html(text) })),
                    )
                    .await?;
                }
                Ok(json!({ "page": v["id"], "title": v["title"] }))
            }
            _ => Err(format!("unknown tool {name:?}").into()),
        }
    }
}

struct Lookups {
    states: Value,
    members: Value,
    labels: Value,
    me: String,
}

impl Lookups {
    fn state_id(&self, name: &str) -> Result<String, ToolErr> {
        self.states
            .as_array()
            .and_then(|a| {
                a.iter().find(|x| {
                    x["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                })
            })
            .map(|x| s(&x["id"]))
            .ok_or_else(|| {
                ToolErr(format!(
                    "no state {name:?}; states: {}",
                    names(&self.states, "name")
                ))
            })
    }
    fn state_name(&self, id: &str) -> String {
        self.states
            .as_array()
            .and_then(|a| a.iter().find(|x| x["id"] == id))
            .map(|x| s(&x["name"]))
            .unwrap_or_default()
    }
    fn user_id(&self, who: &str) -> Result<String, ToolErr> {
        if who.eq_ignore_ascii_case("me") {
            return Ok(self.me.clone());
        }
        self.members
            .as_array()
            .and_then(|a| {
                a.iter().find(|x| {
                    x["email"]
                        .as_str()
                        .is_some_and(|e| e.eq_ignore_ascii_case(who))
                        || x["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case(who))
                })
            })
            .map(|x| s(&x["user_id"]))
            .ok_or_else(|| ToolErr(format!("{who:?} is not a member of this project")))
    }
    fn user_name(&self, id: &str) -> String {
        self.members
            .as_array()
            .and_then(|a| a.iter().find(|x| x["user_id"] == id))
            .map(|x| {
                let n = s(&x["name"]);
                if n.is_empty() { s(&x["email"]) } else { n }
            })
            .unwrap_or_else(|| "someone".into())
    }
    fn label_id(&self, name: &str) -> Result<String, ToolErr> {
        self.labels
            .as_array()
            .and_then(|a| {
                a.iter().find(|x| {
                    x["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                })
            })
            .map(|x| s(&x["id"]))
            .ok_or_else(|| {
                ToolErr(format!(
                    "no label {name:?}; labels: {}",
                    names(&self.labels, "name")
                ))
            })
    }

    fn summary(&self, x: &Value, ident: &str) -> Value {
        json!({
            "key": format!("{ident}-{}", x["sequence"]),
            "title": x["title"],
            "state": self.state_name(x["state_id"].as_str().unwrap_or("")),
            "priority": x["priority"],
            "assignees": x["assignee_ids"].as_array().cloned().unwrap_or_default().iter().map(|u| self.user_name(u.as_str().unwrap_or(""))).collect::<Vec<_>>(),
            "labels": x["label_ids"].as_array().cloned().unwrap_or_default().iter().filter_map(|l| self.labels.as_array()?.iter().find(|y| y["id"] == *l).map(|y| y["name"].clone())).collect::<Vec<_>>(),
            "due_date": x["due_date"],
            "updated_at": x["updated_at"],
        })
    }

    /// Fills a create/update body from the friendly tool arguments.
    async fn apply(
        &self,
        a: &Value,
        body: &mut Value,
        c: &Caller<'_>,
        ws: &str,
    ) -> Result<(), ToolErr> {
        if let Some(d) = a["description"].as_str() {
            body["description_html"] = json!(text_to_html(d));
            body["description_json"] = Value::Null;
        }
        if let Some(st) = a["state"].as_str() {
            body["state_id"] = json!(self.state_id(st)?);
        }
        if let Some(p) = a["priority"].as_str() {
            body["priority"] = json!(p);
        }
        for f in ["due_date", "start_date", "estimate"] {
            if let Some(v) = a.get(f).filter(|v| !v.is_null()) {
                body[f] = v.clone();
            }
        }
        if let Some(list) = a["assignees"].as_array() {
            let mut ids = Vec::new();
            for w in list {
                ids.push(self.user_id(w.as_str().unwrap_or(""))?);
            }
            body["assignee_ids"] = json!(ids);
        }
        if let Some(list) = a["labels"].as_array() {
            let mut ids = Vec::new();
            for l in list {
                ids.push(self.label_id(l.as_str().unwrap_or(""))?);
            }
            body["label_ids"] = json!(ids);
        }
        if let Some(parent) = a["parent"].as_str() {
            let (_, id, _) = c.key(ws, parent).await?;
            body["parent_id"] = json!(id);
        }
        Ok(())
    }
}

fn s(v: &Value) -> String {
    v.as_str()
        .map(String::from)
        .unwrap_or_else(|| v.to_string())
}

fn names(v: &Value, f: &str) -> String {
    v.as_array()
        .map(|a| a.iter().map(|x| s(&x[f])).collect::<Vec<_>>().join(", "))
        .unwrap_or_default()
}

fn urlenc(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Plain text (as agents write it) to safe HTML paragraphs; the server sanitizes it again.
pub fn text_to_html(t: &str) -> String {
    let esc = |x: &str| {
        x.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    t.split("\n\n")
        .map(|p| format!("<p>{}</p>", esc(p.trim()).replace('\n', "<br>")))
        .collect()
}

/// MCP structured content must be an object.
fn wrap(v: Value) -> Value {
    if v.is_object() {
        v
    } else {
        json!({ "result": v })
    }
}

fn text_of(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn tools() -> Value {
    let ws = json!({ "type": "string", "description": "Workspace slug, e.g. acme" });
    let project =
        json!({ "type": "string", "description": "Project identifier (e.g. WEB), name or id" });
    let key = json!({ "type": "string", "description": "Work item key, e.g. WEB-12" });
    let props = json!({
        "description": { "type": "string", "description": "Plain text; blank lines separate paragraphs" },
        "state": { "type": "string", "description": "State name, e.g. In Progress (see list_states)" },
        "priority": { "type": "string", "enum": ["urgent", "high", "medium", "low", "none"] },
        "assignees": { "type": "array", "items": { "type": "string" }, "description": "\"me\", member emails or names; replaces the assignees" },
        "labels": { "type": "array", "items": { "type": "string" }, "description": "Label names; replaces the labels" },
        "start_date": { "type": "string", "description": "YYYY-MM-DD" },
        "due_date": { "type": "string", "description": "YYYY-MM-DD" },
        "estimate": { "type": "string" },
        "parent": { "type": "string", "description": "Key of the parent work item" }
    });
    let mut create = json!({ "workspace": ws, "project": project, "title": { "type": "string" } });
    let mut update = json!({ "workspace": ws, "key": key, "title": { "type": "string" } });
    for (k, v) in props.as_object().unwrap() {
        create[k] = v.clone();
        update[k] = v.clone();
    }
    let ro = json!({ "readOnlyHint": true, "openWorldHint": false });
    json!([
        { "name": "list_workspaces", "description": "Workspaces you belong to.", "inputSchema": { "type": "object", "properties": {} }, "annotations": ro },
        { "name": "list_projects", "description": "Projects in a workspace that you can see, with your role.", "inputSchema": { "type": "object", "properties": { "workspace": ws }, "required": ["workspace"] }, "annotations": ro },
        { "name": "list_states", "description": "Workflow states of a project.", "inputSchema": { "type": "object", "properties": { "workspace": ws, "project": project }, "required": ["workspace", "project"] }, "annotations": ro },
        { "name": "list_members", "description": "Members of a project.", "inputSchema": { "type": "object", "properties": { "workspace": ws, "project": project }, "required": ["workspace", "project"] }, "annotations": ro },
        { "name": "list_labels", "description": "Labels of a project.", "inputSchema": { "type": "object", "properties": { "workspace": ws, "project": project }, "required": ["workspace", "project"] }, "annotations": ro },
        { "name": "list_cycles", "description": "Cycles (sprints) of a project with progress.", "inputSchema": { "type": "object", "properties": { "workspace": ws, "project": project }, "required": ["workspace", "project"] }, "annotations": ro },
        { "name": "list_work_items", "description": "Work items of a project, newest board order first.", "inputSchema": { "type": "object", "properties": {
            "workspace": ws, "project": project,
            "state": { "type": "string" }, "state_group": { "type": "string", "description": "backlog, unstarted, started, completed, cancelled (comma-separated)" },
            "priority": { "type": "string", "description": "comma-separated priorities" },
            "assignee": { "type": "string", "description": "\"me\", an email or a name" },
            "query": { "type": "string", "description": "Words in the title, or a key" },
            "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
        }, "required": ["workspace", "project"] }, "annotations": ro },
        { "name": "get_work_item", "description": "One work item with its description and comments.", "inputSchema": { "type": "object", "properties": { "workspace": ws, "key": key }, "required": ["workspace", "key"] }, "annotations": ro },
        { "name": "list_pages", "description": "Pages (documents) of a project.", "inputSchema": { "type": "object", "properties": { "workspace": ws, "project": project }, "required": ["workspace", "project"] }, "annotations": ro },
        { "name": "get_page", "description": "The text of one page.", "inputSchema": { "type": "object", "properties": { "workspace": ws, "project": project, "page": { "type": "string", "description": "Page id" } }, "required": ["workspace", "project", "page"] }, "annotations": ro },
        { "name": "create_page", "description": "Create a page with a title and plain-text content (blank lines separate paragraphs).", "inputSchema": { "type": "object", "properties": {
            "workspace": ws, "project": project, "title": { "type": "string" }, "text": { "type": "string" }, "parent": { "type": "string", "description": "Parent page id" }
        }, "required": ["workspace", "project", "title"] } },
        { "name": "search", "description": "Full-text search over work item titles, descriptions and comments, and pages.", "inputSchema": { "type": "object", "properties": { "workspace": ws, "query": { "type": "string" } }, "required": ["workspace", "query"] }, "annotations": ro },
        { "name": "create_work_item", "description": "Create a work item.", "inputSchema": { "type": "object", "properties": create, "required": ["workspace", "project", "title"] } },
        { "name": "update_work_item", "description": "Change a work item's title, description, state, priority, assignees, labels, dates, estimate or parent.", "inputSchema": { "type": "object", "properties": update, "required": ["workspace", "key"] } },
        { "name": "add_comment", "description": "Comment on a work item (plain text).", "inputSchema": { "type": "object", "properties": { "workspace": ws, "key": key, "text": { "type": "string" } }, "required": ["workspace", "key", "text"] } }
    ])
}

#[cfg(test)]
mod tests {
    #[test]
    fn text_becomes_escaped_paragraphs() {
        assert_eq!(
            super::text_to_html("Hi <b>\nthere\n\nNext"),
            "<p>Hi &lt;b&gt;<br>there</p><p>Next</p>"
        );
        assert_eq!(super::urlenc("a b/ç"), "a%20b%2F%C3%A7");
    }
}
