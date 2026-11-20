//! CSRF defence in depth (SPEC §9): besides `SameSite=Lax` cookies, a state-changing request
//! that carries an `Origin` header must come from the same host it is sent to. Requests
//! without `Origin` (curl, API tokens) pass; browsers always send it on such requests.

use axum::extract::Request;
use axum::http::{HeaderMap, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use np_core::Error;

use crate::error::ApiError;

pub async fn check_origin(req: Request, next: Next) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe && !same_origin(req.headers()) {
        return ApiError(Error::forbidden("cross-origin request refused")).into_response();
    }
    next.run(req).await
}

/// For WebSocket upgrades (GET requests the middleware lets through): a browser always sends
/// `Origin` on a WebSocket handshake, so another site's page cannot open a socket that rides
/// on the user's cookie.
pub fn websocket_allowed(h: &HeaderMap) -> bool {
    same_origin(h)
}

fn same_origin(h: &HeaderMap) -> bool {
    let Some(origin) = h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    let host = h
        .get("x-forwarded-host")
        .or_else(|| h.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(',')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        });
    let origin_host = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .map(|v| v.trim_end_matches('/').to_ascii_lowercase());
    matches!((origin_host, host), (Some(o), Some(h)) if !h.is_empty() && o == h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut m = HeaderMap::new();
        for (k, v) in pairs {
            m.insert(*k, v.parse().unwrap());
        }
        m
    }

    #[test]
    fn rules() {
        assert!(same_origin(&h(&[("host", "localhost:12313")])));
        assert!(same_origin(&h(&[
            ("host", "localhost:12313"),
            ("origin", "http://localhost:12313")
        ])));
        assert!(same_origin(&h(&[
            ("host", "127.0.0.1"),
            ("x-forwarded-host", "plane.example.com"),
            ("origin", "https://plane.example.com")
        ])));
        assert!(!same_origin(&h(&[
            ("host", "localhost:12313"),
            ("origin", "http://evil.test")
        ])));
        assert!(!same_origin(&h(&[
            ("host", "localhost:12313"),
            ("origin", "null")
        ])));
        assert!(!same_origin(&h(&[("origin", "http://localhost:12313")])));
    }
}
