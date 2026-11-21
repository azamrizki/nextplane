mod common;

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Form, Path, State};
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use serde_json::json;

#[derive(Clone, Default)]
struct Fake {
    base: String,
    nonce: Arc<Mutex<String>>,
    email: Arc<Mutex<String>>,
    primary_verified: Arc<Mutex<bool>>,
    in_org: Arc<Mutex<bool>>,
}

/// One local server playing an OpenID provider (GitLab-like) and GitHub's OAuth and API.
async fn fake() -> Fake {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let f = Fake {
        base: base.clone(),
        primary_verified: Arc::new(Mutex::new(true)),
        in_org: Arc::new(Mutex::new(true)),
        ..Default::default()
    };
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(|State(f): State<Fake>| async move {
            Json(json!({ "issuer": f.base, "authorization_endpoint": format!("{}/oauth/authorize", f.base),
                         "token_endpoint": format!("{}/oauth/token", f.base) }))
        }))
        .route("/oauth/token", post(|State(f): State<Fake>, Form(_): Form<Vec<(String, String)>>| async move {
            let claims = json!({ "iss": f.base, "aud": "lab-client", "exp": 4_000_000_000i64, "nonce": *f.nonce.lock().unwrap(),
                                 "email": *f.email.lock().unwrap(), "email_verified": true, "name": "Lab User" });
            let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string());
            Json(json!({ "access_token": "at", "id_token": format!("e30.{enc}.x") }))
        }))
        .route("/login/oauth/access_token", post(|Form(f): Form<Vec<(String, String)>>| async move {
            let ok = f.iter().any(|(k, v)| k == "client_secret" && v == "hub-secret");
            Json(if ok { json!({ "access_token": "gh-token" }) } else { json!({ "error": "bad_verification_code" }) })
        }))
        .route("/api/v3/user", get(|| async { Json(json!({ "login": "octo", "name": "Octo Cat" })) }))
        .route("/api/v3/user/emails", get(|State(f): State<Fake>| async move {
            let pv = *f.primary_verified.lock().unwrap();
            Json(json!([
                { "email": "other@example.com", "primary": false, "verified": true },
                { "email": *f.email.lock().unwrap(), "primary": true, "verified": pv },
            ]))
        }))
        .route("/api/v3/orgs/{org}/memberships/{login}", get(|State(f): State<Fake>, Path((org, login)): Path<(String, String)>| async move {
            if *f.in_org.lock().unwrap() && org == "acme" && login == "octo" { StatusCode::OK } else { StatusCode::NOT_FOUND }
        }))
        .with_state(f.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    f
}

/// start → (provider) → callback for `/auth/oauth/{p}`; returns the callback's Location and
/// session cookie.
async fn sign_in(t: &common::TestApp, f: &Fake, p: &str, email: &str) -> (String, Option<String>) {
    *f.email.lock().unwrap() = email.into();
    let go = |uri: String| {
        let app = t.app.clone();
        async move {
            tower::ServiceExt::oneshot(
                app,
                Request::get(uri)
                    .header(header::HOST, "localhost:12313")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
        }
    };
    let res = go(format!("/api/v1/auth/oauth/{p}/start?next=/acme")).await;
    let loc = res.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_string();
    assert!(loc.starts_with(&f.base), "{loc}");
    let url = reqwest::Url::parse(&loc).unwrap();
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(
        q["redirect_uri"],
        format!("http://localhost:12313/api/v1/auth/oauth/{p}/callback")
    );
    if let Some(n) = q.get("nonce") {
        *f.nonce.lock().unwrap() = n.clone();
    }
    let res = go(format!(
        "/api/v1/auth/oauth/{p}/callback?code=c0de&state={}",
        q["state"]
    ))
    .await;
    let location = res
        .headers()
        .get(header::LOCATION)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string());
    (location, cookie)
}

#[tokio::test]
async fn admins_set_up_providers_and_people_sign_in_with_them() {
    let f = fake().await;
    let t = common::app().await;
    let ana = t.setup_admin().await;
    let a = t.get("/api/v1/admin/auth", Some(&ana)).await.body;
    assert_eq!(a["password_enabled"], true);
    assert_eq!(a["providers"].as_array().unwrap().len(), 4);
    assert_eq!(
        a["providers"][2]["callback_url"],
        "http://localhost/api/v1/auth/oauth/gitlab/callback"
    );
    // Not before it is set up; secrets are never sent back.
    assert_eq!(
        t.patch(
            "/api/v1/admin/auth",
            json!({ "providers": { "github": { "enabled": true } } }),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let r = t
        .patch(
            "/api/v1/admin/auth",
            json!({ "providers": {
                "gitlab": { "client_id": "lab-client", "client_secret": "lab-secret", "host": format!("{}/", f.base), "enabled": true },
                "github": { "client_id": "hub-client", "client_secret": "hub-secret", "host": f.base, "org": "acme", "enabled": true },
            } }),
            Some(&ana),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(!r.body.to_string().contains("lab-secret"));
    assert_eq!(
        r.body["providers"][2]["host"], f.base,
        "trailing slash dropped"
    );
    assert_eq!(r.body["providers"][2]["has_secret"], true);
    let info = t.get("/api/v1/instance", None).await.body;
    assert_eq!(
        info["providers"],
        json!([{ "key": "github", "name": "GitHub" }, { "key": "gitlab", "name": "GitLab" }])
    );
    // Someone else cannot read or change it.
    t.workspace(&ana, "acme").await;
    let bob = t.join(&ana, "acme", "bob@example.com", "member").await;
    assert_eq!(
        t.get("/api/v1/admin/auth", Some(&bob)).await.status,
        StatusCode::FORBIDDEN
    );

    // GitLab (OpenID Connect): an existing account by its verified email.
    let (loc, cookie) = sign_in(&t, &f, "gitlab", "Bob@Example.com").await;
    assert_eq!(loc, "/acme");
    assert_eq!(
        t.get("/api/v1/me", cookie.as_deref()).await.body["email"],
        "bob@example.com"
    );

    // GitHub: the primary email must be verified and the person in the organization.
    let (loc, cookie) = sign_in(&t, &f, "github", "bob@example.com").await;
    assert_eq!(loc, "/acme");
    assert!(cookie.is_some());
    *f.in_org.lock().unwrap() = false;
    let (loc, cookie) = sign_in(&t, &f, "github", "bob@example.com").await;
    assert!(loc.contains("acme%20organization"), "{loc}");
    assert!(cookie.is_none());
    *f.in_org.lock().unwrap() = true;
    *f.primary_verified.lock().unwrap() = false;
    let (loc, _) = sign_in(&t, &f, "github", "bob@example.com").await;
    assert!(loc.contains("not%20verified"), "{loc}");
    *f.primary_verified.lock().unwrap() = true;
    // Strangers need the signup policy or an invite.
    let (loc, _) = sign_in(&t, &f, "github", "stranger@example.com").await;
    assert!(loc.contains("no%20account"), "{loc}");

    // A provider that is off cannot be used.
    t.patch(
        "/api/v1/admin/auth",
        json!({ "providers": { "gitlab": { "enabled": false } } }),
        Some(&ana),
    )
    .await;
    let res = tower::ServiceExt::oneshot(
        t.app.clone(),
        Request::get("/api/v1/auth/oauth/gitlab/start")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert!(
        res.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .contains("not%20set%20up")
    );
}

#[tokio::test]
async fn password_sign_in_can_be_turned_off_but_not_every_way_in() {
    let f = fake().await;
    let t = common::app().await;
    let ana = t.setup_admin().await;
    t.workspace(&ana, "acme").await;
    t.join(&ana, "acme", "bob@example.com", "member").await;
    // Not while it is the only way in.
    assert_eq!(
        t.patch(
            "/api/v1/admin/auth",
            json!({ "password_enabled": false }),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    let r = t
        .patch(
            "/api/v1/admin/auth",
            json!({ "password_enabled": false, "providers": { "github": { "client_id": "c", "client_secret": "hub-secret", "host": f.base, "enabled": true } } }),
            Some(&ana),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        t.get("/api/v1/instance", None).await.body["password_enabled"],
        false
    );
    // People sign in with the provider now; instance admins keep their password as a way back.
    let r = t
        .post(
            "/api/v1/auth/login",
            json!({ "email": "bob@example.com", "password": "password123" }),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    let r = t
        .post(
            "/api/v1/auth/login",
            json!({ "email": "ana@example.com", "password": "correct horse" }),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    // Turning the last provider off is refused too.
    assert_eq!(
        t.patch(
            "/api/v1/admin/auth",
            json!({ "providers": { "github": { "enabled": false } } }),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
}
