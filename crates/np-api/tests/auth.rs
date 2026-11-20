mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;

#[tokio::test]
async fn instance_reports_setup_required_until_setup() {
    let t = common::app().await;
    let r = t.get("/api/v1/instance", None).await;
    assert_eq!(r.body["setup_required"], true);
    t.setup_admin().await;
    let r = t.get("/api/v1/instance", None).await;
    assert_eq!(r.body["setup_required"], false);
    assert_eq!(r.body["name"], "Acme");
}

#[tokio::test]
async fn setup_creates_admin_and_logs_in() {
    let t = common::app().await;
    let token = np_api::issue_setup_token(&t.store).await.unwrap();
    let r = t
        .post("/api/v1/setup/verify", json!({ "token": token }), None)
        .await;
    assert_eq!(r.status, StatusCode::OK);

    let r = t
        .post(
            "/api/v1/setup",
            json!({ "token": token, "instance_name": " Acme ", "name": "Ana",
                    "email": "Ana@Example.com", "password": "correct horse" }),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["email"], "ana@example.com");
    assert_eq!(r.body["is_instance_admin"], true);
    assert!(r.body.get("password_hash").is_none());
    let set = r.set_cookie.clone().unwrap();
    assert!(set.contains("HttpOnly") && set.contains("SameSite=Lax") && set.contains("Path=/"));
    assert!(!set.contains("Secure"));

    let me = t.get("/api/v1/me", Some(&r.cookie())).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["email"], "ana@example.com");

    // The token is single use and setup cannot run twice.
    let again = t
        .post(
            "/api/v1/setup",
            json!({ "token": token, "instance_name": "X", "name": "", "email": "x@example.com", "password": "12345678" }),
            None,
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert!(np_api::issue_setup_token(&t.store).await.is_err());
}

#[tokio::test]
async fn setup_rejects_bad_token_and_bad_input() {
    let t = common::app().await;
    let token = np_api::issue_setup_token(&t.store).await.unwrap();
    let base = json!({ "token": token, "instance_name": "Acme", "name": "Ana",
                       "email": "ana@example.com", "password": "correct horse" });

    let mut wrong = base.clone();
    wrong["token"] = json!("nope");
    assert_eq!(
        t.post("/api/v1/setup", wrong, None).await.status,
        StatusCode::FORBIDDEN
    );

    for (field, value) in [
        ("email", "nope"),
        ("password", "short"),
        ("instance_name", "  "),
    ] {
        let mut b = base.clone();
        b[field] = json!(value);
        let r = t.post("/api/v1/setup", b, None).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{field}");
        assert_eq!(r.body["error"]["code"], "bad_request");
    }

    // A newer link replaces the older one.
    let newer = np_api::issue_setup_token(&t.store).await.unwrap();
    assert_eq!(
        t.post("/api/v1/setup", base, None).await.status,
        StatusCode::FORBIDDEN
    );
    let mut ok = json!({ "token": newer, "instance_name": "Acme", "name": "Ana",
                         "email": "ana@example.com", "password": "correct horse" });
    ok["token"] = json!(newer);
    assert_eq!(
        t.post("/api/v1/setup", ok, None).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn login_logout_and_me() {
    let t = common::app().await;
    t.setup_admin().await;

    assert_eq!(
        t.get("/api/v1/me", None).await.status,
        StatusCode::UNAUTHORIZED
    );

    let bad = t
        .post(
            "/api/v1/auth/login",
            json!({ "email": "ana@example.com", "password": "nope nope" }),
            None,
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
    let unknown = t
        .post(
            "/api/v1/auth/login",
            json!({ "email": "bob@example.com", "password": "nope nope" }),
            None,
        )
        .await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        bad.body, unknown.body,
        "same answer for unknown email and wrong password"
    );

    let ok = t
        .post(
            "/api/v1/auth/login",
            json!({ "email": " ANA@example.com", "password": "correct horse" }),
            None,
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);
    let cookie = ok.cookie();
    assert_eq!(
        t.get("/api/v1/me", Some(&cookie)).await.status,
        StatusCode::OK
    );

    let out = t
        .post("/api/v1/auth/logout", json!({}), Some(&cookie))
        .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert!(out.set_cookie.unwrap().contains("Max-Age=0"));
    assert_eq!(
        t.get("/api/v1/me", Some(&cookie)).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn secure_cookie_behind_https_proxy() {
    let t = common::app().await;
    t.setup_admin().await;
    let req = Request::post("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-proto", "https")
        .body(Body::from(
            json!({ "email": "ana@example.com", "password": "correct horse" }).to_string(),
        ))
        .unwrap();
    let r = t.send(req).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.set_cookie.unwrap().contains("Secure"));
}

#[tokio::test]
async fn login_is_rate_limited_per_email() {
    let t = common::app().await;
    t.setup_admin().await;
    let mut last = StatusCode::OK;
    for _ in 0..11 {
        last = t
            .post(
                "/api/v1/auth/login",
                json!({ "email": "ana@example.com", "password": "wrong pass" }),
                None,
            )
            .await
            .status;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    // Even the right password is refused while limited.
    let r = t
        .post(
            "/api/v1/auth/login",
            json!({ "email": "ana@example.com", "password": "correct horse" }),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn cross_origin_posts_are_refused() {
    let t = common::app().await;
    let req = Request::post("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::HOST, "localhost:12313")
        .header(header::ORIGIN, "http://evil.test")
        .body(Body::from(
            json!({ "email": "a@b.co", "password": "x" }).to_string(),
        ))
        .unwrap();
    let r = t.send(req).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body["error"]["code"], "forbidden");
}
