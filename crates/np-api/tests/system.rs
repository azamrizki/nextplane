mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};

#[tokio::test]
async fn health_reports_ok_and_schema_version() {
    let t = common::app().await;
    let r = t.get("/api/v1/health", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["status"], "ok");
    assert!(r.body["schema_version"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn version_reports_build_info() {
    let t = common::app().await;
    let r = t.get("/api/v1/version", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["version"], "1.2.3");
    assert_eq!(r.body["commit"], "abc1234");
}

#[tokio::test]
async fn wrong_method_returns_json_405() {
    let t = common::app().await;
    let r = t
        .send(Request::post("/api/v1/health").body(Body::empty()).unwrap())
        .await;
    assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(r.body["error"]["code"], "method_not_allowed");
}

#[tokio::test]
async fn unknown_api_routes_return_json_404() {
    let t = common::app().await;
    for path in ["/api/v1/nope", "/api/v2/health", "/api"] {
        let r = t.get(path, None).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(r.body["error"]["code"], "not_found", "{path}");
    }
}

#[tokio::test]
async fn malformed_requests_get_json_400() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    // Wrong JSON type, bad query value, bad path id: all answer in the JSON error format.
    let r = t
        .post(
            "/api/v1/workspaces",
            serde_json::json!({ "name": 5 }),
            Some(&ana),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.body["error"]["code"], "bad_request");
    t.workspace(&ana, "acme").await;
    let r = t
        .get(
            "/api/v1/workspaces/acme/projects?archived=maybe",
            Some(&ana),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.body["error"]["code"], "bad_request");
    let r = t
        .get("/api/v1/workspaces/acme/projects/not-a-uuid", Some(&ana))
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.body["error"]["code"], "bad_request");
}
