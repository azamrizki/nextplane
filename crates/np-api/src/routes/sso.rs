//! `GET /auth/oidc/start` and `GET /auth/oidc/callback`.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use np_core::user;
use np_store::instance::SignupPolicy;
use serde::Deserialize;

use crate::auth::{self, ClientIp};
use crate::extract::Query;
use crate::oidc::{self, Discovery};
use crate::{AppState, ratelimit};

fn fail(msg: &str) -> Response {
    let enc: String = msg
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    Redirect::to(&format!("/login?sso_error={enc}")).into_response()
}

fn redirect_uri(state: &AppState, headers: &HeaderMap) -> String {
    format!("{}/api/v1/auth/oidc/callback", state.base_url(headers))
}

#[derive(Deserialize)]
pub struct StartQuery {
    next: Option<String>,
}

pub async fn start(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Query(q): Query<StartQuery>,
) -> Response {
    let Some(cfg) = state.oidc.clone() else {
        return fail("single sign-on is not set up on this server");
    };
    if state
        .limiter
        .check(&format!("sso:{ip}"), ratelimit::LOGIN_PER_IP)
        .is_err()
    {
        return fail("too many attempts, try again later");
    }
    let d: Discovery = match oidc::discover(&state.http, &cfg.issuer).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "OIDC discovery failed");
            return fail("the sign-in provider cannot be reached");
        }
    };
    let (st, nonce, verifier) = (
        auth::random_token(),
        auth::random_token(),
        auth::random_token(),
    );
    state.oidc_pending.put(
        &st,
        &verifier,
        &nonce,
        &oidc::safe_next(q.next.as_deref().unwrap_or("/")),
    );
    let mut url = match reqwest::Url::parse(&d.authorization_endpoint) {
        Ok(u) => u,
        Err(_) => return fail("the sign-in provider is misconfigured"),
    };
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", &redirect_uri(&state, &headers))
        .append_pair("scope", &cfg.scopes)
        .append_pair("state", &st)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &oidc::challenge(&verifier))
        .append_pair("code_challenge_method", "S256");
    Redirect::to(url.as_str()).into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    id_token: Option<String>,
}

#[derive(Deserialize)]
struct UserInfo {
    email: Option<String>,
    email_verified: Option<serde_json::Value>,
    name: Option<String>,
}

pub async fn callback(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    jar: CookieJar,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let Some(cfg) = state.oidc.clone() else {
        return fail("single sign-on is not set up on this server");
    };
    if let Some(e) = q.error {
        return fail(&q.error_description.unwrap_or(e));
    }
    let (Some(code), Some(st)) = (q.code, q.state) else {
        return fail("the sign-in answer was incomplete");
    };
    let Some((verifier, nonce, next)) = state.oidc_pending.take(&st) else {
        return fail("this sign-in expired; try again");
    };
    let result: anyhow::Result<(String, String)> = async {
        let d = oidc::discover(&state.http, &cfg.issuer).await?;
        let tok: TokenResponse = state
            .http
            .post(&d.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", redirect_uri(&state, &headers).as_str()),
                ("client_id", cfg.client_id.as_str()),
                ("client_secret", cfg.client_secret.as_str()),
                ("code_verifier", verifier.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let id_token = tok
            .id_token
            .ok_or_else(|| anyhow::anyhow!("the provider sent no ID token"))?;
        let claims = oidc::check_id_token(
            &id_token,
            &cfg,
            &nonce,
            time::OffsetDateTime::now_utc().unix_timestamp(),
        )?;
        let (mut email, mut ok, mut name) = (
            claims.email,
            oidc::verified(&claims.email_verified),
            claims.name,
        );
        if (email.is_none() || !ok)
            && let (Some(ui), Some(at)) = (&d.userinfo_endpoint, &tok.access_token)
        {
            let info: UserInfo = state
                .http
                .get(ui)
                .bearer_auth(at)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            email = info.email.or(email);
            ok = ok || oidc::verified(&info.email_verified);
            name = name.or(info.name);
        }
        let email =
            email.ok_or_else(|| anyhow::anyhow!("the provider did not share an email address"))?;
        if !ok {
            anyhow::bail!("the provider has not verified this email address");
        }
        Ok((email, name.unwrap_or_default()))
    }
    .await;
    let (email, name) = match result {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "OIDC sign-in failed");
            // Our own checks have messages meant for people; transport errors stay in the log.
            let msg = if e.is::<reqwest::Error>() {
                "the sign-in provider did not answer as expected".to_string()
            } else {
                e.to_string()
            };
            return fail(&msg);
        }
    };
    let Ok(email) = user::normalize_email(&email) else {
        return fail("the provider sent an unusable email address");
    };
    let user = match state.store.user_by_email(&email).await {
        Ok(Some(u)) if !u.is_bot => u,
        Ok(Some(_)) => return fail("this account cannot sign in"),
        Ok(None) => {
            let policy = state
                .store
                .instance()
                .await
                .map(|i| i.signup_policy)
                .unwrap_or(SignupPolicy::Closed);
            let invited = state
                .store
                .has_pending_invite(&email)
                .await
                .unwrap_or(false);
            let allowed =
                policy == SignupPolicy::Open || (policy == SignupPolicy::Invite && invited);
            if !allowed {
                return fail(&format!("there is no account for {email} on this server"));
            }
            let name = user::normalize_name(&name, user::NAME_MAX, "name").unwrap_or_default();
            match state.store.create_user(&email, &name, None, false).await {
                Ok(u) => u,
                Err(e) => return fail(&e.message),
            }
        }
        Err(_) => return fail("sign-in failed"),
    };
    match auth::start_session(&state, jar, &headers, &ip, user.id).await {
        Ok(jar) => {
            tracing::info!(email = %user.email, "signed in with OIDC");
            (jar, Redirect::to(&next)).into_response()
        }
        Err(_) => fail("sign-in failed"),
    }
}
