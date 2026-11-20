mod common;

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Form, State};
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use serde_json::{Value, json};

#[derive(Clone, Default)]
struct Provider {
    issuer: String,
    nonce: Arc<Mutex<String>>,
    email: Arc<Mutex<String>>,
    verified: Arc<Mutex<bool>>,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

/// A tiny OpenID provider on a random local port.
async fn provider() -> Provider {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let p = Provider {
        issuer: issuer.clone(),
        verified: Arc::new(Mutex::new(true)),
        ..Default::default()
    };
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(|State(p): State<Provider>| async move {
            Json(json!({ "issuer": p.issuer, "authorization_endpoint": format!("{}/authorize", p.issuer),
                         "token_endpoint": format!("{}/token", p.issuer), "userinfo_endpoint": format!("{}/userinfo", p.issuer) }))
        }))
        .route("/token", post(|State(p): State<Provider>, Form(f): Form<Vec<(String, String)>>| async move {
            *p.seen.lock().unwrap() = f;
            let claims = json!({ "iss": p.issuer, "aud": "np-client", "exp": 4_000_000_000i64, "nonce": *p.nonce.lock().unwrap(),
                                 "email": *p.email.lock().unwrap(), "email_verified": *p.verified.lock().unwrap(), "name": "Sso User" });
            let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string());
            Json(json!({ "access_token": "at", "id_token": format!("e30.{enc}.x"), "token_type": "Bearer" }))
        }))
        .route("/userinfo", get(|State(p): State<Provider>| async move {
            Json(json!({ "email": *p.email.lock().unwrap(), "email_verified": *p.verified.lock().unwrap() }))
        }))
        .with_state(p.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    p
}

async fn sso_app(p: &Provider) -> common::TestApp {
    common::app_with(|s| {
        s.oidc = Some(np_api::oidc::OidcConfig {
            issuer: p.issuer.clone(),
            client_id: "np-client".into(),
            client_secret: "secret".into(),
            name: "Test SSO".into(),
            scopes: "openid email profile".into(),
        })
    })
    .await
}

/// Runs start → (provider) → callback; returns the callback's status, Location and cookie.
async fn sign_in(
    t: &common::TestApp,
    p: &Provider,
    email: &str,
) -> (StatusCode, String, Option<String>) {
    *p.email.lock().unwrap() = email.into();
    let r = t.app.clone();
    let res = tower::ServiceExt::oneshot(
        r.clone(),
        Request::get("/api/v1/auth/oidc/start?next=/acme/inbox")
            .header(header::HOST, "localhost:12313")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let loc = res.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        loc.starts_with(&format!("{}/authorize?", p.issuer)),
        "{loc}"
    );
    let url = reqwest::Url::parse(&loc).unwrap();
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(
        q["redirect_uri"],
        "http://localhost:12313/api/v1/auth/oidc/callback"
    );
    *p.nonce.lock().unwrap() = q["nonce"].clone();
    let cb = format!("/api/v1/auth/oidc/callback?code=xyz&state={}", q["state"]);
    let res = tower::ServiceExt::oneshot(
        r,
        Request::get(&cb)
            .header(header::HOST, "localhost:12313")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    let status = res.status();
    let location = res
        .headers()
        .get(header::LOCATION)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string());
    (status, location, cookie)
}

#[tokio::test]
async fn existing_account_signs_in_with_pkce() {
    let p = provider().await;
    let t = sso_app(&p).await;
    t.setup_admin().await;
    assert_eq!(
        t.get("/api/v1/instance", None).await.body["sso_name"],
        "Test SSO"
    );
    let (status, loc, cookie) = sign_in(&t, &p, "Ana@Example.com").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(loc, "/acme/inbox");
    let me = t.get("/api/v1/me", cookie.as_deref()).await.body;
    assert_eq!(me["email"], "ana@example.com");
    let seen: Value = json!(
        p.seen
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<std::collections::HashMap<_, _>>()
    );
    assert_eq!(seen["grant_type"], "authorization_code");
    assert!(seen["code_verifier"].as_str().unwrap().len() >= 43);
}

#[tokio::test]
async fn unknown_people_and_unverified_emails_are_refused() {
    let p = provider().await;
    let t = sso_app(&p).await;
    let ana = t.setup_admin().await;
    let (_, loc, cookie) = sign_in(&t, &p, "stranger@example.com").await;
    assert!(loc.starts_with("/login?sso_error="), "{loc}");
    assert!(cookie.is_none());

    // An invite lets a new person in (policy "invite").
    t.workspace(&ana, "acme").await;
    t.post(
        "/api/v1/workspaces/acme/invites",
        json!({ "invites": [{ "email": "new@example.com", "role": "member" }] }),
        Some(&ana),
    )
    .await;
    let (_, loc, cookie) = sign_in(&t, &p, "new@example.com").await;
    assert_eq!(loc, "/acme/inbox");
    assert_eq!(
        t.get("/api/v1/me", cookie.as_deref()).await.body["name"],
        "Sso User"
    );

    *p.verified.lock().unwrap() = false;
    let (_, loc, _) = sign_in(&t, &p, "ana@example.com").await;
    assert!(loc.contains("verified"), "{loc}");

    // A replayed or unknown state is refused.
    let res = tower::ServiceExt::oneshot(
        t.app.clone(),
        Request::get("/api/v1/auth/oidc/callback?code=x&state=nope")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert!(
        res.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .contains("expired")
    );
}
