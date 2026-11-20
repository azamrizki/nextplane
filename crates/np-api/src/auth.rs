//! Passwords, random tokens, the session cookie and the `CurrentUser` extractor.

use std::net::SocketAddr;
use std::sync::OnceLock;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use np_core::{Error, ErrorKind, User};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};

use crate::AppState;
use crate::error::ApiError;

pub const SESSION_COOKIE: &str = "np_session";
pub const SESSION_TTL: Duration = Duration::days(30);
/// A session's expiry is pushed forward at most once per this interval.
const SESSION_TOUCH_EVERY: Duration = Duration::hours(1);
pub const SETUP_TOKEN_TTL: Duration = Duration::minutes(30);

/// 32 random bytes, base64url without padding (43 characters).
pub fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("OS random source");
    URL_SAFE_NO_PAD.encode(buf)
}

fn new_salt() -> SaltString {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("OS random source");
    SaltString::encode_b64(&buf).expect("16 bytes is a valid salt length")
}

pub fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// argon2id with the crate's default parameters, run off the async threads.
pub async fn hash_password(password: String) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || {
        let salt = new_salt();
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
    })
    .await
    .map_err(|e| anyhow::anyhow!("hash task: {e}"))?
    .map_err(|e| anyhow::anyhow!("hash password: {e}").into())
}

/// Checks a password against a stored hash. With no hash (unknown user, or a user without a
/// password) a dummy hash is checked anyway, so the response time does not reveal which.
pub async fn verify_password(password: String, stored: Option<String>) -> bool {
    static DUMMY: OnceLock<String> = OnceLock::new();
    tokio::task::spawn_blocking(move || {
        let has_hash = stored.is_some();
        let hash = stored.unwrap_or_else(|| {
            DUMMY
                .get_or_init(|| {
                    let salt = new_salt();
                    Argon2::default()
                        .hash_password(random_token().as_bytes(), &salt)
                        .map(|h| h.to_string())
                        .unwrap_or_default()
                })
                .clone()
        });
        let ok = PasswordHash::new(&hash)
            .map(|h| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &h)
                    .is_ok()
            })
            .unwrap_or(false);
        ok && has_hash
    })
    .await
    .unwrap_or(false)
}

pub fn expires_in(d: Duration) -> String {
    np_store::fmt_ts(OffsetDateTime::now_utc() + d)
}

/// True when the browser reached us over HTTPS (directly or through a proxy that says so).
pub fn is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

pub fn session_cookie(token: String, secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(SESSION_TTL)
        .build()
}

pub fn clear_session_cookie(secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(Duration::ZERO)
        .build()
}

pub fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|c| Cookie::parse(c.trim().to_string()).ok())
        .find(|c| c.name() == SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .filter(|v| !v.is_empty())
}

/// The client address: the first `X-Forwarded-For` hop when the peer is a local proxy,
/// otherwise the peer itself.
pub fn client_ip(parts_headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    let peer_ip = peer.map(|p| p.ip());
    let behind_local_proxy = peer_ip.is_none_or(|ip| ip.is_loopback());
    if behind_local_proxy
        && let Some(first) = parts_headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(str::trim)
            .filter(|v| !v.is_empty())
    {
        return first.to_string();
    }
    peer_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".into())
}

pub fn peer_addr(parts: &Parts) -> Option<SocketAddr> {
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0)
}

/// The logged-in user. Rejects with `401 unauthorized` when there is no live session.
#[derive(Debug, Clone)]
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let unauthorized = || ApiError(Error::new(ErrorKind::Unauthorized, "log in first"));
        // API tokens: `Authorization: Bearer np_…`. A read token may only read.
        if let Some(bearer) = bearer_token(&parts.headers) {
            let (user, scope) = state
                .store
                .token_user(&token_hash(&bearer))
                .await?
                .ok_or_else(|| {
                    ApiError(Error::new(
                        ErrorKind::Unauthorized,
                        "this API token is not valid",
                    ))
                })?;
            let reading = matches!(
                parts.method,
                axum::http::Method::GET | axum::http::Method::HEAD
            );
            if scope == "read" && !reading {
                return Err(Error::forbidden("this API token can only read").into());
            }
            return Ok(CurrentUser(user));
        }
        let token = cookie_token(&parts.headers).ok_or_else(unauthorized)?;
        let (session_id, last_seen, user) = state
            .store
            .session_user(&token_hash(&token))
            .await?
            .ok_or_else(unauthorized)?;
        let stale = np_store::parse_ts(&last_seen)
            .is_none_or(|t| OffsetDateTime::now_utc() - t > SESSION_TOUCH_EVERY);
        if stale {
            state
                .store
                .touch_session(session_id, &expires_in(SESSION_TTL))
                .await?;
        }
        Ok(CurrentUser(user))
    }
}

/// Creates a session for `user_id` and adds its cookie to the jar.
pub async fn start_session(
    state: &AppState,
    jar: axum_extra::extract::CookieJar,
    headers: &HeaderMap,
    ip: &str,
    user_id: uuid::Uuid,
) -> Result<axum_extra::extract::CookieJar, ApiError> {
    let token = random_token();
    let ua: String = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(300)
        .collect();
    state
        .store
        .create_session(
            user_id,
            &token_hash(&token),
            &expires_in(SESSION_TTL),
            &ua,
            ip,
        )
        .await?;
    Ok(jar.add(session_cookie(token, is_https(headers))))
}

/// The client's IP as text (see [`client_ip`]).
pub struct ClientIp(pub String);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientIp(client_ip(&parts.headers, peer_addr(parts))))
    }
}

pub const TOKEN_PREFIX: &str = "np_";

/// `Authorization: Bearer np_…`
pub fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|t| t.starts_with(TOKEN_PREFIX))
        .map(String::from)
}

/// The user's id for a request, without touching the session (for event metadata).
pub async fn peek_user_id(state: &AppState, headers: &HeaderMap) -> Option<uuid::Uuid> {
    if let Some(b) = bearer_token(headers) {
        return state
            .store
            .token_user(&token_hash(&b))
            .await
            .ok()
            .flatten()
            .map(|(u, _)| u.id);
    }
    let token = cookie_token(headers)?;
    state
        .store
        .session_user(&token_hash(&token))
        .await
        .ok()
        .flatten()
        .map(|(_, _, u)| u.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_roundtrip() {
        let h = hash_password("correct horse".into()).await.unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("correct horse".into(), Some(h.clone())).await);
        assert!(!verify_password("wrong horse".into(), Some(h)).await);
        assert!(!verify_password("anything".into(), None).await);
    }

    #[test]
    fn tokens_are_random_and_hashed() {
        let a = random_token();
        assert_eq!(a.len(), 43);
        assert_ne!(a, random_token());
        assert_eq!(token_hash(&a).len(), 32);
    }

    #[test]
    fn client_ip_trusts_forwarded_only_from_loopback() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.9, 10.0.0.1".parse().unwrap());
        assert_eq!(
            client_ip(&h, Some("127.0.0.1:5000".parse().unwrap())),
            "203.0.113.9"
        );
        assert_eq!(
            client_ip(&h, Some("198.51.100.2:5000".parse().unwrap())),
            "198.51.100.2"
        );
    }
}
