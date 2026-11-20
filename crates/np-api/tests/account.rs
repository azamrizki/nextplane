mod common;

use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn open_signup_only_when_policy_is_open() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    let body = json!({ "email": "z@example.com", "name": "Z", "password": "password123" });
    assert_eq!(
        t.post("/api/v1/auth/signup", body.clone(), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    t.patch(
        "/api/v1/admin/settings",
        json!({ "signup_policy": "open" }),
        Some(&ana),
    )
    .await;
    let r = t.post("/api/v1/auth/signup", body.clone(), None).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    assert_eq!(
        t.get("/api/v1/me", Some(&r.cookie())).await.body["email"],
        "z@example.com"
    );
    assert_eq!(
        t.post("/api/v1/auth/signup", body, None).await.status,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn profile_and_password_change() {
    let t = common::app().await;
    t.setup_admin().await;
    let bob = t.user("bob@example.com").await;
    let other_session = t.login("bob@example.com", "password123").await;

    let r = t
        .patch("/api/v1/me", json!({ "name": "  Bobby " }), Some(&bob))
        .await;
    assert_eq!(r.body["name"], "Bobby");

    let wrong = t
        .post(
            "/api/v1/me/password",
            json!({ "current": "nope nope", "new": "newpassword1" }),
            Some(&bob),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    let short = t
        .post(
            "/api/v1/me/password",
            json!({ "current": "password123", "new": "short" }),
            Some(&bob),
        )
        .await;
    assert_eq!(short.status, StatusCode::BAD_REQUEST);
    let ok = t
        .post(
            "/api/v1/me/password",
            json!({ "current": "password123", "new": "newpassword1" }),
            Some(&bob),
        )
        .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT);
    assert_eq!(
        t.get("/api/v1/me", Some(&bob)).await.status,
        StatusCode::OK,
        "this session stays"
    );
    assert_eq!(
        t.get("/api/v1/me", Some(&other_session)).await.status,
        StatusCode::UNAUTHORIZED,
        "others end"
    );
    t.login("bob@example.com", "newpassword1").await;
}

#[tokio::test]
async fn admin_reset_link_flow() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    let bob = t.user("bob@example.com").await;
    let bob_id = t.user_id(&bob).await;

    assert_eq!(
        t.post(
            &format!("/api/v1/admin/users/{bob_id}/reset-link"),
            json!({}),
            Some(&bob)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let r = t
        .post(
            &format!("/api/v1/admin/users/{bob_id}/reset-link"),
            json!({}),
            Some(&ana),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let link = r.body["link"].as_str().unwrap();
    assert!(link.starts_with("http://localhost:12313/reset/"));
    let token = link.rsplit('/').next().unwrap();

    assert_eq!(
        t.get(&format!("/api/v1/auth/reset/{token}"), None)
            .await
            .body["email"],
        "bob@example.com"
    );
    let r = t
        .post(
            &format!("/api/v1/auth/reset/{token}"),
            json!({ "password": "brandnew123" }),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(
        t.get("/api/v1/me", Some(&bob)).await.status,
        StatusCode::UNAUTHORIZED,
        "old sessions end"
    );
    assert_eq!(
        t.get(&format!("/api/v1/auth/reset/{token}"), None)
            .await
            .status,
        StatusCode::NOT_FOUND,
        "single use"
    );
    t.login("bob@example.com", "brandnew123").await;
}

#[tokio::test]
async fn forgot_always_answers_204() {
    let t = common::app().await;
    t.setup_admin().await;
    for e in ["ana@example.com", "nobody@example.com", "bad"] {
        assert_eq!(
            t.post("/api/v1/auth/forgot", json!({ "email": e }), None)
                .await
                .status,
            StatusCode::NO_CONTENT
        );
    }
}

#[tokio::test]
async fn instance_admin_users_and_last_admin_rule() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    let bob = t.user("bob@example.com").await;
    let (ana_id, bob_id) = (t.user_id(&ana).await, t.user_id(&bob).await);

    assert_eq!(
        t.get("/api/v1/admin/users", Some(&bob)).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.get("/api/v1/admin/users", Some(&ana))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let demote = t
        .patch(
            &format!("/api/v1/admin/users/{ana_id}"),
            json!({ "is_instance_admin": false }),
            Some(&ana),
        )
        .await;
    assert_eq!(demote.status, StatusCode::CONFLICT);
    let promote = t
        .patch(
            &format!("/api/v1/admin/users/{bob_id}"),
            json!({ "is_instance_admin": true }),
            Some(&ana),
        )
        .await;
    assert_eq!(promote.body["is_instance_admin"], true);
    let demote = t
        .patch(
            &format!("/api/v1/admin/users/{ana_id}"),
            json!({ "is_instance_admin": false }),
            Some(&ana),
        )
        .await;
    assert_eq!(demote.status, StatusCode::OK);

    let s = t.get("/api/v1/admin/settings", Some(&bob)).await;
    assert_eq!(s.body["signup_policy"], "invite");
    assert_eq!(s.body["email_enabled"], false);
    assert_eq!(
        t.patch("/api/v1/admin/settings", json!({ "name": "" }), Some(&bob))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    t.workspace(&bob, "acme").await;
    let ws = t.get("/api/v1/admin/workspaces", Some(&bob)).await;
    assert_eq!(ws.body[0]["member_count"], 1);
}
