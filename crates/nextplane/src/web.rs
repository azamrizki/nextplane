//! The web app, embedded from `web/dist` at build time (`make web` first). Hashed files under
//! `/assets/` are cached for a year; every other unknown path gets `index.html` so the React
//! router can handle it.

use axum::Router;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
#[exclude = ".gitkeep"]
struct Assets;

pub fn router() -> Router {
    Router::new().fallback(serve)
}

async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if !path.is_empty() {
        if let Some(file) = Assets::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let cache = if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            return file_response(file.data.into_owned(), mime.as_ref(), cache);
        }
        // A missing hashed asset is a real 404, not the app shell.
        if path.starts_with("assets/") {
            return StatusCode::NOT_FOUND.into_response();
        }
    }
    match Assets::get("index.html") {
        Some(index) => file_response(
            index.data.into_owned(),
            "text/html; charset=utf-8",
            "no-cache",
        ),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "NextPlane: the web app is not built into this binary. Run `make build`.",
        )
            .into_response(),
    }
}

fn file_response(body: Vec<u8>, mime: &str, cache: &'static str) -> Response {
    let mut res = body.into_response();
    let h = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(mime) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert("referrer-policy", HeaderValue::from_static("same-origin"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    res
}
