//! The AI writing assistant and Unsplash photos (S3.7), once an instance admin set them up.

use axum::extract::State;
use np_core::Error;
use np_core::perm::WsRole;
use serde::{Deserialize, Serialize};

use super::workspaces::load;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};
use crate::services::{self, Photo};
use crate::{AppState, ratelimit};

#[derive(Deserialize)]
pub struct AiReq {
    task: String,
    #[serde(default)]
    prompt: String,
}

#[derive(Serialize)]
pub struct AiOut {
    response: String,
    /// The answer as simple HTML (paragraphs and line breaks), ready for the editor.
    response_html: String,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Plain text to paragraphs: blank lines split paragraphs, single newlines become `<br>`.
fn to_html(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| format!("<p>{}</p>", esc(p).replace('\n', "<br>")))
        .collect()
}

/// `POST /workspaces/{slug}/ai`: workspace members and admins (not guests), 60 an hour each.
pub async fn ai(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<AiReq>,
) -> ApiResult<Json<AiOut>> {
    let ws = load(&state, &slug, &me).await?;
    if ws.role == WsRole::Guest {
        return Err(Error::forbidden("guests cannot use the AI assistant").into());
    }
    let ai = services::ai(&state.store).await?;
    if !ai.enabled() {
        return Err(Error::bad_request("the AI assistant is not set up on this server").into());
    }
    let task = req.task.trim();
    if task.is_empty() {
        return Err(Error::bad_request("tell the assistant what to do").into());
    }
    if task.len() > 4000 || req.prompt.len() > 20_000 {
        return Err(Error::bad_request("that is too long for the assistant").into());
    }
    state
        .limiter
        .check(&format!("ai:{}", me.id), ratelimit::AI_PER_USER)?;
    let response = services::complete(&state.http, &ai, task, req.prompt.trim())
        .await
        .map_err(|e| {
            tracing::warn!(error = %format!("{e:#}"), "AI assistant failed");
            Error::new(np_core::ErrorKind::Internal, "the AI service did not answer; try again later")
        })?;
    Ok(Json(AiOut {
        response_html: to_html(&response),
        response,
    }))
}

#[derive(Deserialize)]
pub struct PhotosQuery {
    query: Option<String>,
    page: Option<u32>,
}

/// `GET /workspaces/{slug}/unsplash`: latest photos or a search; empty when Unsplash is off.
pub async fn photos(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Query(q): Query<PhotosQuery>,
) -> ApiResult<Json<Vec<Photo>>> {
    load(&state, &slug, &me).await?;
    let key = services::unsplash_key(&state.store).await?;
    if key.is_empty() {
        return Ok(Json(Vec::new()));
    }
    state
        .limiter
        .check(&format!("unsplash:{}", me.id), ratelimit::UNSPLASH_PER_USER)?;
    let page = q.page.unwrap_or(1).clamp(1, 50);
    let list = services::unsplash_photos(&state.http, &state.unsplash_api, &key, q.query.as_deref(), page)
        .await
        .map_err(|e| {
            tracing::warn!(error = %format!("{e:#}"), "Unsplash failed");
            Error::new(np_core::ErrorKind::Internal, "Unsplash did not answer; try again later")
        })?;
    Ok(Json(list))
}

#[cfg(test)]
mod tests {
    #[test]
    fn answers_become_paragraphs() {
        assert_eq!(
            super::to_html("Steps:\n1. a <b>\n\n\nDone & dusted"),
            "<p>Steps:<br>1. a &lt;b&gt;</p><p>Done &amp; dusted</p>"
        );
    }
}
