//! Single sign-on: the generic OpenID provider of config.toml (`/auth/oidc/…`) and the providers
//! the instance admin turns on (`/auth/oauth/{google|github|gitlab|gitea}/…`, S3.7). Everyone is
//! matched to an account by a verified email address.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use np_core::user;
use np_store::instance::SignupPolicy;
use serde::Deserialize;

use crate::auth::{self, ClientIp};
use crate::extract::{Path, Query};
use crate::oidc::{self, Discovery, OidcConfig};
use crate::providers::{self, Provider};
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

fn redirect_uri(state: &AppState, headers: &HeaderMap, provider: Option<&str>) -> String {
    match provider {
        Some(p) => format!("{}/api/v1/auth/oauth/{p}/callback", state.base_url(headers)),
        None => format!("{}/api/v1/auth/oidc/callback", state.base_url(headers)),
    }
}

#[derive(Deserialize)]
pub struct StartQuery {
    next: Option<String>,
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// The provider a login attempt belongs to travels with it, so a callback for one provider
/// cannot finish another's attempt.
fn tag(provider: &str, next: &str) -> String {
    format!("{provider}\n{next}")
}

fn untag(provider: &str, tagged: &str) -> Option<String> {
    let (p, next) = tagged.split_once('\n')?;
    (p == provider).then(|| next.to_string())
}

fn limited(state: &AppState, ip: &str) -> bool {
    state
        .limiter
        .check(&format!("sso:{ip}"), ratelimit::LOGIN_PER_IP)
        .is_err()
}

// ---- config.toml's OpenID provider -----------------------------------------------------------

pub async fn start(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Query(q): Query<StartQuery>,
) -> Response {
    let Some(cfg) = state.oidc.clone() else {
        return fail("single sign-on is not set up on this server");
    };
    if limited(&state, &ip) {
        return fail("too many attempts, try again later");
    }
    oidc_start(
        &state,
        &cfg,
        &redirect_uri(&state, &headers, None),
        "oidc",
        q.next.as_deref(),
    )
    .await
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
    let redirect = redirect_uri(&state, &headers, None);
    let (code, next, verifier, nonce) = match begin_callback(&state, "oidc", q) {
        Ok(x) => x,
        Err(r) => return r,
    };
    let who = oidc_identity(&state, &cfg, &code, &verifier, &nonce, &redirect).await;
    finish(&state, jar, &headers, &ip, who, &next, "OIDC").await
}

// ---- Providers the instance admin turns on ---------------------------------------------------

async fn usable(state: &AppState, key: &str) -> Result<Provider, Response> {
    match providers::find(&state.store, key).await {
        Ok(Some(p)) if p.usable() => Ok(p),
        Ok(Some(p)) => Err(fail(&format!(
            "sign-in with {} is not set up on this server",
            p.name()
        ))),
        _ => Err(fail("there is no such sign-in provider")),
    }
}

pub async fn oauth_start(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Path(key): Path<String>,
    Query(q): Query<StartQuery>,
) -> Response {
    let p = match usable(&state, &key).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    if limited(&state, &ip) {
        return fail("too many attempts, try again later");
    }
    let redirect = redirect_uri(&state, &headers, Some(p.key));
    if let Some(cfg) = p.oidc() {
        return oidc_start(&state, &cfg, &redirect, p.key, q.next.as_deref()).await;
    }
    // GitHub: plain OAuth (it has no OpenID Connect for people's accounts).
    let st = auth::random_token();
    state.oidc_pending.put(
        &st,
        "",
        "",
        &tag(p.key, &oidc::safe_next(q.next.as_deref().unwrap_or("/"))),
    );
    let (web, _) = p.github_bases();
    let Ok(mut url) = reqwest::Url::parse(&format!("{web}/login/oauth/authorize")) else {
        return fail("the sign-in provider is misconfigured");
    };
    let scope = if p.org.is_empty() {
        "read:user user:email"
    } else {
        "read:user user:email read:org"
    };
    url.query_pairs_mut()
        .append_pair("client_id", &p.client_id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("scope", scope)
        .append_pair("state", &st)
        .append_pair("allow_signup", "false");
    Redirect::to(url.as_str()).into_response()
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    jar: CookieJar,
    Path(key): Path<String>,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let p = match usable(&state, &key).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let redirect = redirect_uri(&state, &headers, Some(p.key));
    let (code, next, verifier, nonce) = match begin_callback(&state, p.key, q) {
        Ok(x) => x,
        Err(r) => return r,
    };
    let who = match p.oidc() {
        Some(cfg) => oidc_identity(&state, &cfg, &code, &verifier, &nonce, &redirect).await,
        None => github_identity(&state, &p, &code, &redirect).await,
    };
    finish(&state, jar, &headers, &ip, who, &next, p.name()).await
}

// ---- Shared steps ----------------------------------------------------------------------------

async fn oidc_start(
    state: &AppState,
    cfg: &OidcConfig,
    redirect: &str,
    provider: &str,
    next: Option<&str>,
) -> Response {
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
        &tag(provider, &oidc::safe_next(next.unwrap_or("/"))),
    );
    let mut url = match reqwest::Url::parse(&d.authorization_endpoint) {
        Ok(u) => u,
        Err(_) => return fail("the sign-in provider is misconfigured"),
    };
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", redirect)
        .append_pair("scope", &cfg.scopes)
        .append_pair("state", &st)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &oidc::challenge(&verifier))
        .append_pair("code_challenge_method", "S256");
    Redirect::to(url.as_str()).into_response()
}

/// Checks the provider's answer and takes the pending attempt: (code, next, verifier, nonce).
#[allow(clippy::result_large_err)]
fn begin_callback(
    state: &AppState,
    provider: &str,
    q: CallbackQuery,
) -> Result<(String, String, String, String), Response> {
    if let Some(e) = q.error {
        return Err(fail(&q.error_description.unwrap_or(e)));
    }
    let (Some(code), Some(st)) = (q.code, q.state) else {
        return Err(fail("the sign-in answer was incomplete"));
    };
    let Some((verifier, nonce, tagged)) = state.oidc_pending.take(&st) else {
        return Err(fail("this sign-in expired; try again"));
    };
    let Some(next) = untag(provider, &tagged) else {
        return Err(fail("this sign-in expired; try again"));
    };
    Ok((code, next, verifier, nonce))
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

/// (email, name) from an OpenID provider; the email must be verified.
async fn oidc_identity(
    state: &AppState,
    cfg: &OidcConfig,
    code: &str,
    verifier: &str,
    nonce: &str,
    redirect: &str,
) -> anyhow::Result<(String, String)> {
    let d = oidc::discover(&state.http, &cfg.issuer).await?;
    let tok: TokenResponse = state
        .http
        .post(&d.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect),
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
            ("code_verifier", verifier),
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
        cfg,
        nonce,
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

#[derive(Deserialize)]
struct GithubUser {
    login: String,
    name: Option<String>,
}

#[derive(Deserialize)]
struct GithubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

/// (email, name) from GitHub: the primary email, which must be verified (as in Plane), and
/// membership of the organization when one is set.
async fn github_identity(
    state: &AppState,
    p: &Provider,
    code: &str,
    redirect: &str,
) -> anyhow::Result<(String, String)> {
    let (web, api) = p.github_bases();
    let tok: TokenResponse = state
        .http
        .post(format!("{web}/login/oauth/access_token"))
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[
            ("client_id", p.client_id.as_str()),
            ("client_secret", p.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", redirect),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let at = tok
        .access_token
        .ok_or_else(|| anyhow::anyhow!("GitHub did not accept the sign-in"))?;
    let get = |path: String| {
        state
            .http
            .get(format!("{api}{path}"))
            .bearer_auth(&at)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
    };
    let user: GithubUser = get("/user".into())
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let emails: Vec<GithubEmail> = get("/user/emails".into())
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let email = emails
        .into_iter()
        .find(|e| e.primary && e.verified)
        .ok_or_else(|| anyhow::anyhow!("your primary GitHub email address is not verified"))?
        .email;
    if !p.org.is_empty() {
        let r = get(format!("/orgs/{}/memberships/{}", p.org, user.login))
            .send()
            .await?;
        if !r.status().is_success() {
            anyhow::bail!(
                "only members of the {} organization on GitHub may sign in",
                p.org
            );
        }
    }
    Ok((email, user.name.unwrap_or(user.login)))
}

/// Signs in the account with this email, creating one when the signup policy allows it.
async fn finish(
    state: &AppState,
    jar: CookieJar,
    headers: &HeaderMap,
    ip: &str,
    who: anyhow::Result<(String, String)>,
    next: &str,
    via: &str,
) -> Response {
    let (email, name) = match who {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), provider = via, "single sign-on failed");
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
    match auth::start_session(state, jar, headers, ip, user.id).await {
        Ok(jar) => {
            tracing::info!(email = %user.email, provider = via, "signed in with single sign-on");
            (jar, Redirect::to(next)).into_response()
        }
        Err(_) => fail("sign-in failed"),
    }
}
