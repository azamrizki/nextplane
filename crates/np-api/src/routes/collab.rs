//! Comments (with @mentions), reactions, files, links and subscriptions (PERMISSIONS §2.1).

use axum::body::Body;
use axum::extract::{Multipart, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use np_core::id::{Uuid, new_id};
use np_core::perm::{Ownership, ProjAction, WsAction, ws_require};
use np_core::{Error, User};
use np_store::collab::{Comment, FileMeta, Link, NewFile, ReactionCount};
use serde::Deserialize;

use super::projects::{ProjCtx, load_project};
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::extract::{Json, Path};
use crate::richtext;

type ItemPath = Path<(String, Uuid, Uuid)>;

/// Loads the project and checks the work item exists and can be seen.
async fn item_ctx(
    state: &AppState,
    me: &User,
    slug: &str,
    pid: Uuid,
    wid: Uuid,
) -> ApiResult<ProjCtx> {
    let ctx = load_project(state, slug, pid, me).await?;
    ctx.require(ProjAction::ViewWorkItems)?;
    state
        .store
        .work_item(pid, wid)
        .await?
        .ok_or_else(|| Error::not_found("work item not found"))?;
    Ok(ctx)
}

fn own(owner: Uuid, me: &User) -> Ownership {
    if owner == me.id {
        Ownership::Own
    } else {
        Ownership::Others
    }
}

/// `data-id` of every `<span data-type="mention">` in sanitized HTML.
pub fn mentioned_ids(html: &str) -> Vec<Uuid> {
    let mut out = Vec::new();
    for part in html.split("<span").skip(1) {
        let tag = part.split('>').next().unwrap_or("");
        if !tag.contains("data-type=\"mention\"") {
            continue;
        }
        if let Some(rest) = tag.split("data-id=\"").nth(1)
            && let Ok(id) = rest.split('"').next().unwrap_or("").parse::<Uuid>()
            && !out.contains(&id)
        {
            out.push(id);
        }
    }
    out
}

// ---- comments ---------------------------------------------------------------------------

pub async fn comments(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
) -> ApiResult<Json<Vec<Comment>>> {
    item_ctx(&state, &me, &slug, pid, wid).await?;
    Ok(Json(state.store.comments(wid).await?))
}

#[derive(Deserialize)]
pub struct CommentReq {
    body_json: Option<serde_json::Value>,
    body_html: String,
}

fn clean_comment(req: CommentReq) -> ApiResult<(Option<String>, String)> {
    let (json, html) = richtext::clean_body(req.body_json, &req.body_html)?;
    if richtext::plain_text(&html).is_empty() && !html.contains("<img") {
        return Err(Error::bad_request("a comment cannot be empty").into());
    }
    Ok((json, html))
}

pub async fn add_comment(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
    Json(req): Json<CommentReq>,
) -> ApiResult<(StatusCode, Json<Comment>)> {
    let ctx = item_ctx(&state, &me, &slug, pid, wid).await?;
    ctx.require_write(ProjAction::Comment)?;
    let (json, html) = clean_comment(req)?;
    let c = state
        .store
        .add_comment(wid, me.id, json.as_deref(), &html)
        .await?;
    state.store.subscribe(wid, me.id).await?;
    // Mentioned people who can see the project follow the item from now on.
    let mentioned = mentioned_ids(&html);
    for u in &mentioned {
        if state.store.proj_role(pid, *u).await?.is_some()
            || state
                .store
                .ws_role(ctx.ws.workspace.id, *u)
                .await?
                .is_some_and(|r| r.is_admin())
        {
            state.store.subscribe(wid, *u).await?;
        }
    }
    state
        .store
        .notify_comment(wid, me.id, c.id, &mentioned)
        .await?;
    Ok((StatusCode::CREATED, Json(c)))
}

async fn load_comment(state: &AppState, wid: Uuid, cid: Uuid) -> ApiResult<Comment> {
    Ok(state
        .store
        .comment(wid, cid)
        .await?
        .ok_or_else(|| Error::not_found("comment not found"))?)
}

pub async fn edit_comment(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid, cid)): Path<(String, Uuid, Uuid, Uuid)>,
    Json(req): Json<CommentReq>,
) -> ApiResult<Json<Comment>> {
    let ctx = item_ctx(&state, &me, &slug, pid, wid).await?;
    let c = load_comment(&state, wid, cid).await?;
    ctx.require_write(ProjAction::EditComment(own(c.author_id, &me)))?;
    let (json, html) = clean_comment(req)?;
    state
        .store
        .edit_comment(cid, json.as_deref(), &html)
        .await?;
    Ok(Json(load_comment(&state, wid, cid).await?))
}

pub async fn delete_comment(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid, cid)): Path<(String, Uuid, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let ctx = item_ctx(&state, &me, &slug, pid, wid).await?;
    let c = load_comment(&state, wid, cid).await?;
    ctx.require_write(ProjAction::EditComment(own(c.author_id, &me)))?;
    state.store.delete_comment(cid).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_hidden(
    state: AppState,
    me: User,
    slug: String,
    pid: Uuid,
    wid: Uuid,
    cid: Uuid,
    hidden: bool,
) -> ApiResult<Json<Comment>> {
    let ctx = item_ctx(&state, &me, &slug, pid, wid).await?;
    load_comment(&state, wid, cid).await?;
    ctx.require_write(ProjAction::HideComment)?;
    state.store.set_comment_hidden(cid, me.id, hidden).await?;
    Ok(Json(load_comment(&state, wid, cid).await?))
}

pub async fn hide_comment(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid, cid)): Path<(String, Uuid, Uuid, Uuid)>,
) -> ApiResult<Json<Comment>> {
    set_hidden(state, me, slug, pid, wid, cid, true).await
}

pub async fn unhide_comment(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid, cid)): Path<(String, Uuid, Uuid, Uuid)>,
) -> ApiResult<Json<Comment>> {
    set_hidden(state, me, slug, pid, wid, cid, false).await
}

// ---- reactions --------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReactReq {
    emoji: String,
}

fn clean_emoji(e: &str) -> ApiResult<String> {
    let e = e.trim();
    if e.is_empty()
        || e.chars().count() > 8
        || e.chars()
            .any(|c| c.is_ascii_alphanumeric() || c.is_whitespace() || c == '<')
    {
        return Err(Error::bad_request("a reaction is one emoji").into());
    }
    Ok(e.to_string())
}

pub async fn react_comment(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid, cid)): Path<(String, Uuid, Uuid, Uuid)>,
    Json(req): Json<ReactReq>,
) -> ApiResult<Json<Vec<ReactionCount>>> {
    let ctx = item_ctx(&state, &me, &slug, pid, wid).await?;
    load_comment(&state, wid, cid).await?;
    ctx.require_write(ProjAction::React)?;
    Ok(Json(
        state
            .store
            .toggle_reaction("comment", cid, me.id, &clean_emoji(&req.emoji)?)
            .await?,
    ))
}

pub async fn item_reactions(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
) -> ApiResult<Json<Vec<ReactionCount>>> {
    item_ctx(&state, &me, &slug, pid, wid).await?;
    Ok(Json(state.store.reactions("work_item", wid).await?))
}

pub async fn react_item(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
    Json(req): Json<ReactReq>,
) -> ApiResult<Json<Vec<ReactionCount>>> {
    let ctx = item_ctx(&state, &me, &slug, pid, wid).await?;
    ctx.require_write(ProjAction::React)?;
    Ok(Json(
        state
            .store
            .toggle_reaction("work_item", wid, me.id, &clean_emoji(&req.emoji)?)
            .await?,
    ))
}

// ---- files ------------------------------------------------------------------------------

/// Types the browser may show inline. Everything else downloads (SPEC §9).
const INLINE_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "application/pdf",
    "text/plain",
];
const IMAGE_TYPES: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

struct Upload {
    name: String,
    mime: String,
    bytes: axum::body::Bytes,
}

/// Reads the single `file` field of a multipart body.
async fn read_upload(state: &AppState, mut mp: Multipart) -> ApiResult<Upload> {
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| Error::bad_request(e.body_text()))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let name = field.file_name().unwrap_or("file").to_string();
        let bytes = field
            .bytes()
            .await
            .map_err(|e| Error::bad_request(e.body_text()))?;
        if bytes.len() > state.max_upload {
            return Err(Error::bad_request(format!(
                "files can be at most {} MB",
                state.max_upload / 1024 / 1024
            ))
            .into());
        }
        if bytes.is_empty() {
            return Err(Error::bad_request("the file is empty").into());
        }
        let name = clean_file_name(&name);
        let mime = mime_guess::from_path(&name)
            .first_or_octet_stream()
            .essence_str()
            .to_string();
        return Ok(Upload { name, mime, bytes });
    }
    Err(Error::bad_request("send the file in a multipart field named \"file\"").into())
}

fn clean_file_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("file");
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '<' | '>'))
        .take(200)
        .collect();
    if cleaned.trim().is_empty() || cleaned == "." || cleaned == ".." {
        "file".into()
    } else {
        cleaned
    }
}

/// Writes bytes under `<uploads>/<yyyy-mm>/<id>` and records them.
async fn store_file(
    state: &AppState,
    me: &User,
    up: Upload,
    mut f: NewFile,
) -> ApiResult<FileMeta> {
    let month = &np_store::now_ts()[..7];
    let dir = state.uploads_dir.join(month);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| anyhow::anyhow!("create {}: {e}", dir.display()))?;
    let id = new_id();
    let rel = format!("{month}/{}", id.simple());
    let path = state.uploads_dir.join(&rel);
    tokio::fs::write(&path, &up.bytes)
        .await
        .map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))?;
    f.file_name = up.name;
    f.mime = up.mime;
    f.size = up.bytes.len() as i64;
    f.path = rel;
    Ok(state.store.add_file(&f, me.id).await?)
}

pub async fn attachments(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
) -> ApiResult<Json<Vec<FileMeta>>> {
    item_ctx(&state, &me, &slug, pid, wid).await?;
    Ok(Json(state.store.work_item_files(wid).await?))
}

pub async fn upload_attachment(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
    mp: Multipart,
) -> ApiResult<(StatusCode, Json<FileMeta>)> {
    let ctx = item_ctx(&state, &me, &slug, pid, wid).await?;
    ctx.require_write(ProjAction::AddAttachment)?;
    let up = read_upload(&state, mp).await?;
    let f = NewFile {
        workspace_id: Some(ctx.ws.workspace.id),
        project_id: Some(pid),
        work_item_id: Some(wid),
        purpose: "attachment",
        ..Default::default()
    };
    Ok((
        StatusCode::CREATED,
        Json(store_file(&state, &me, up, f).await?),
    ))
}

pub async fn delete_attachment(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid, fid)): Path<(String, Uuid, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let ctx = item_ctx(&state, &me, &slug, pid, wid).await?;
    let f = state
        .store
        .file(fid)
        .await?
        .filter(|f| f.work_item_id == Some(wid))
        .ok_or_else(|| Error::not_found("attachment not found"))?;
    ctx.require_write(ProjAction::DeleteAttachment(own(f.uploaded_by, &me)))?;
    state.store.delete_file(fid).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// An image pasted or dropped into a description or comment.
pub async fn upload_editor_image(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    mp: Multipart,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let ctx = load_project(&state, &slug, pid, &me).await?;
    ctx.require_write(ProjAction::Comment)?;
    let up = read_upload(&state, mp).await?;
    if !IMAGE_TYPES.contains(&up.mime.as_str()) {
        return Err(
            Error::bad_request("only PNG, JPEG, GIF and WebP images can be embedded").into(),
        );
    }
    let f = NewFile {
        workspace_id: Some(ctx.ws.workspace.id),
        project_id: Some(pid),
        purpose: "editor",
        ..Default::default()
    };
    let meta = store_file(&state, &me, up, f).await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": meta.id, "url": format!("/api/v1/files/{}", meta.id) })),
    ))
}

pub async fn upload_avatar(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    mp: Multipart,
) -> ApiResult<Json<User>> {
    let up = read_upload(&state, mp).await?;
    if !IMAGE_TYPES.contains(&up.mime.as_str()) || up.bytes.len() > 2 * 1024 * 1024 {
        return Err(
            Error::bad_request("avatars are PNG, JPEG, GIF or WebP images up to 2 MB").into(),
        );
    }
    let meta = store_file(
        &state,
        &me,
        up,
        NewFile {
            purpose: "avatar",
            ..Default::default()
        },
    )
    .await?;
    let url = format!("/api/v1/files/{}", meta.id);
    state
        .store
        .update_profile(me.id, None, Some(Some(&url)))
        .await?;
    Ok(Json(
        state
            .store
            .user(me.id)
            .await?
            .ok_or_else(|| Error::not_found("user not found"))?,
    ))
}

pub async fn upload_logo(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    mp: Multipart,
) -> ApiResult<Json<serde_json::Value>> {
    let ws = super::workspaces::load(&state, &slug, &me).await?;
    ws_require(
        ws.role,
        WsAction::EditSettings,
        super::workspaces::facts(&ws.workspace),
    )?;
    let up = read_upload(&state, mp).await?;
    if !IMAGE_TYPES.contains(&up.mime.as_str()) || up.bytes.len() > 2 * 1024 * 1024 {
        return Err(
            Error::bad_request("logos are PNG, JPEG, GIF or WebP images up to 2 MB").into(),
        );
    }
    let meta = store_file(
        &state,
        &me,
        up,
        NewFile {
            workspace_id: Some(ws.workspace.id),
            purpose: "logo",
            ..Default::default()
        },
    )
    .await?;
    let url = format!("/api/v1/files/{}", meta.id);
    state
        .store
        .update_workspace(
            ws.workspace.id,
            &np_store::workspaces::WorkspacePatch {
                logo: Some(Some(url.clone())),
                ..Default::default()
            },
        )
        .await?;
    Ok(Json(serde_json::json!({ "logo": url })))
}

/// Serves a file to someone allowed to see what it belongs to.
pub async fn download(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(fid): Path<Uuid>,
) -> ApiResult<Response> {
    let f = state
        .store
        .file(fid)
        .await?
        .ok_or_else(|| Error::not_found("file not found"))?;
    let allowed = match f.purpose.as_str() {
        "avatar" => true,
        "logo" => match f.workspace_id {
            Some(ws) => state.store.ws_role(ws, me.id).await?.is_some(),
            None => false,
        },
        _ => match (f.workspace_id, f.project_id) {
            (Some(ws), Some(pid)) => can_view_project(&state, &me, ws, pid).await?,
            _ => false,
        },
    };
    if !allowed {
        return Err(Error::not_found("file not found").into());
    }
    let path = state.uploads_dir.join(&f.path);
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        tracing::warn!(path = %path.display(), error = %e, "file missing on disk");
        Error::not_found("file not found")
    })?;
    let inline = INLINE_TYPES.contains(&f.mime.as_str());
    let mut res = Response::new(Body::from(bytes));
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&f.mime)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    let disp = format!(
        "{}; filename=\"{}\"; filename*=UTF-8''{}",
        if inline { "inline" } else { "attachment" },
        f.file_name
            .replace(|c: char| !c.is_ascii() || c == '"' || c == '\\', "_"),
        utf8_percent(&f.file_name)
    );
    if let Ok(v) = HeaderValue::from_str(&disp) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "sandbox; default-src 'none'; img-src 'self'; style-src 'unsafe-inline'",
        ),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    Ok(res.into_response())
}

async fn can_view_project(state: &AppState, me: &User, ws: Uuid, pid: Uuid) -> ApiResult<bool> {
    let Some(ws_role) = state.store.ws_role(ws, me.id).await? else {
        return Ok(false);
    };
    let explicit = state.store.proj_role(pid, me.id).await?;
    let role = np_core::perm::effective_proj_role(ws_role, explicit);
    Ok(role.is_some_and(|r| np_core::perm::proj_can(r, ProjAction::ViewWorkItems)))
}

fn utf8_percent(s: &str) -> String {
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

// ---- links ------------------------------------------------------------------------------

pub async fn links(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
) -> ApiResult<Json<Vec<Link>>> {
    item_ctx(&state, &me, &slug, pid, wid).await?;
    Ok(Json(state.store.links_of(wid).await?))
}

#[derive(Deserialize)]
pub struct LinkReq {
    url: String,
    #[serde(default)]
    title: String,
}

pub async fn add_link(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
    Json(req): Json<LinkReq>,
) -> ApiResult<Json<Vec<Link>>> {
    let ctx = item_ctx(&state, &me, &slug, pid, wid).await?;
    ctx.require_write(ProjAction::EditWorkItem)?;
    let url = req.url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://"))
        || url.len() > 2048
        || url.chars().any(char::is_whitespace)
    {
        return Err(Error::bad_request("links start with http:// or https://").into());
    }
    let title: String = req.title.trim().chars().take(200).collect();
    state.store.add_link(wid, url, &title, me.id).await?;
    Ok(Json(state.store.links_of(wid).await?))
}

pub async fn remove_link(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid, lid)): Path<(String, Uuid, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let ctx = item_ctx(&state, &me, &slug, pid, wid).await?;
    ctx.require_write(ProjAction::EditWorkItem)?;
    state
        .store
        .link(wid, lid)
        .await?
        .ok_or_else(|| Error::not_found("link not found"))?;
    state.store.remove_link(wid, lid).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- subscriptions ----------------------------------------------------------------------

pub async fn subscription(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
) -> ApiResult<Json<serde_json::Value>> {
    item_ctx(&state, &me, &slug, pid, wid).await?;
    Ok(Json(
        serde_json::json!({ "subscribed": state.store.is_subscribed(wid, me.id).await? }),
    ))
}

pub async fn subscribe(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
) -> ApiResult<Json<serde_json::Value>> {
    item_ctx(&state, &me, &slug, pid, wid).await?;
    state.store.subscribe(wid, me.id).await?;
    Ok(Json(serde_json::json!({ "subscribed": true })))
}

pub async fn unsubscribe(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid, wid)): ItemPath,
) -> ApiResult<Json<serde_json::Value>> {
    item_ctx(&state, &me, &slug, pid, wid).await?;
    state.store.unsubscribe(wid, me.id).await?;
    Ok(Json(serde_json::json!({ "subscribed": false })))
}

#[cfg(test)]
mod tests {
    #[test]
    fn mentions_and_names() {
        let id = "0190a4e8-0000-7000-8000-000000000001";
        let html = format!(
            r#"<p>hi <span data-type="mention" data-id="{id}" data-label="Ana">@Ana</span> and <span class="x">no</span></p>"#
        );
        assert_eq!(super::mentioned_ids(&html).len(), 1);
        assert_eq!(super::clean_file_name("../../etc/passwd"), "passwd");
        assert_eq!(super::clean_file_name("C:\\x\\a\"b<.png"), "ab.png");
        assert_eq!(super::clean_file_name(".."), "file");
    }
}
