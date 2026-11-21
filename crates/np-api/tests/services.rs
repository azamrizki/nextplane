mod common;

use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

#[derive(Clone, Default)]
struct Fake {
    base: String,
    last_ai: Arc<Mutex<Option<(String, Value)>>>,
    downloads: Arc<Mutex<Vec<String>>>,
}

/// One local server playing an OpenAI-compatible API and Unsplash.
async fn fake() -> Fake {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let f = Fake {
        base: format!("http://{}", listener.local_addr().unwrap()),
        ..Default::default()
    };
    let photo = |id: &str, base: &str| {
        json!({ "id": id, "alt_description": "a calm sea", "urls": { "small": format!("https://images.unsplash.com/{id}-s"), "regular": format!("https://images.unsplash.com/{id}-r") },
                "user": { "name": "Sam Shore", "links": { "html": "https://unsplash.com/@sam" } },
                "links": { "download_location": format!("{base}/photos/{id}/download") } })
    };
    let app = Router::new()
        .route("/v1/chat/completions", post(|State(f): State<Fake>, h: HeaderMap, Json(b): Json<Value>| async move {
            let auth = h.get("authorization").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
            *f.last_ai.lock().unwrap() = Some((auth, b));
            Json(json!({ "choices": [{ "message": { "role": "assistant", "content": "Steps:\n1. Reproduce\n\nThen fix it." } }] }))
        }))
        .route("/search/photos", get(move |State(f): State<Fake>, h: HeaderMap, Query(q): Query<Vec<(String, String)>>| async move {
            if h.get("authorization").map(|v| v.to_str().unwrap()) != Some("Client-ID uk") {
                return (StatusCode::UNAUTHORIZED, Json(json!({})));
            }
            let query = q.iter().find(|(k, _)| k == "query").map(|(_, v)| v.clone()).unwrap_or_default();
            (StatusCode::OK, Json(json!({ "results": [photo(&format!("{query}1"), &f.base)] })))
        }))
        .route("/photos", get(move |State(f): State<Fake>| async move { Json(json!([photo("latest1", &f.base), photo("latest2", &f.base)])) }))
        .route("/photos/{id}/download", get(|State(f): State<Fake>, axum::extract::Path(id): axum::extract::Path<String>| async move {
            f.downloads.lock().unwrap().push(id);
            Json(json!({ "url": "x" }))
        }))
        .with_state(f.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    f
}

#[tokio::test]
async fn the_ai_assistant_once_an_admin_sets_it_up() {
    let f = fake().await;
    let t = common::app().await;
    let ana = t.setup_admin().await;
    t.workspace(&ana, "acme").await;
    let s = t.get("/api/v1/admin/services", Some(&ana)).await.body;
    assert_eq!(
        (s["ai"]["enabled"].as_bool(), s["ai"]["model"].as_str()),
        (Some(false), Some("gpt-4o-mini"))
    );
    let ask =
        json!({ "task": "Write a description for this work item", "prompt": "Fix the login page" });
    assert_eq!(
        t.post("/api/v1/workspaces/acme/ai", ask.clone(), Some(&ana))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    let r = t
        .patch(
            "/api/v1/admin/services",
            json!({ "ai": { "api_key": "sk-secret", "model": "small-1", "base_url": format!("{}/v1/", f.base) } }),
            Some(&ana),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(!r.body.to_string().contains("sk-secret"));
    assert_eq!(r.body["ai"]["has_key"], true);
    assert_eq!(
        t.get("/api/v1/instance", None).await.body["ai_enabled"],
        true
    );

    let r = t.post("/api/v1/workspaces/acme/ai", ask, Some(&ana)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.body["response_html"],
        "<p>Steps:<br>1. Reproduce</p><p>Then fix it.</p>"
    );
    let (auth, body) = f.last_ai.lock().unwrap().clone().unwrap();
    assert_eq!(auth, "Bearer sk-secret");
    assert_eq!(body["model"], "small-1");
    assert!(
        body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .ends_with("work item\nFix the login page")
    );
    assert_eq!(
        t.post(
            "/api/v1/workspaces/acme/ai",
            json!({ "task": " " }),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let gus = t.join(&ana, "acme", "gus@example.com", "guest").await;
    assert_eq!(
        t.post(
            "/api/v1/workspaces/acme/ai",
            json!({ "task": "x" }),
            Some(&gus)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    // Removing the key turns it off again; only instance admins see or change it.
    assert_eq!(
        t.get("/api/v1/admin/services", Some(&gus)).await.status,
        StatusCode::FORBIDDEN
    );
    t.patch(
        "/api/v1/admin/services",
        json!({ "ai": { "api_key": "" } }),
        Some(&ana),
    )
    .await;
    assert_eq!(
        t.get("/api/v1/instance", None).await.body["ai_enabled"],
        false
    );
}

#[tokio::test]
async fn unsplash_photos_become_credited_project_covers() {
    let f = fake().await;
    let base = f.base.clone();
    let t = common::app_with(move |s| s.unsplash_api = base).await;
    let ana = t.setup_admin().await;
    t.workspace(&ana, "acme").await;
    let photos = "/api/v1/workspaces/acme/unsplash";
    assert_eq!(
        t.get(photos, Some(&ana)).await.body,
        json!([]),
        "off until a key is saved"
    );
    t.patch(
        "/api/v1/admin/services",
        json!({ "unsplash": { "access_key": "uk" } }),
        Some(&ana),
    )
    .await;
    let latest = t.get(photos, Some(&ana)).await.body;
    assert_eq!(latest.as_array().unwrap().len(), 2);
    let found = t.get(&format!("{photos}?query=sea"), Some(&ana)).await.body;
    assert_eq!(found[0]["id"], "sea1");
    assert_eq!(found[0]["author"], "Sam Shore");
    assert_eq!(found[0]["regular"], "https://images.unsplash.com/sea1-r");

    let p = t
        .post(
            "/api/v1/workspaces/acme/projects",
            json!({ "name": "Website" }),
            Some(&ana),
        )
        .await
        .body;
    let url = format!(
        "/api/v1/workspaces/acme/projects/{}",
        p["id"].as_str().unwrap()
    );
    // An Unsplash cover needs its credit; other addresses are refused.
    assert_eq!(
        t.patch(
            &url,
            json!({ "cover_image": "https://images.unsplash.com/sea1-r" }),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.patch(
            &url,
            json!({ "cover_image": "https://evil.example/x.png" }),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let r = t
        .patch(
            &url,
            json!({ "cover_image": "https://images.unsplash.com/sea1-r", "cover_credit": { "name": "Sam Shore", "url": "https://unsplash.com/@sam" },
                    "unsplash_download": found[0]["download_location"] }),
            Some(&ana),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["cover_image"], "https://images.unsplash.com/sea1-r");
    assert_eq!(r.body["cover_credit"]["name"], "Sam Shore");
    // Unsplash hears the photo was used (in the background).
    for _ in 0..50 {
        if !f.downloads.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(*f.downloads.lock().unwrap(), ["sea1"]);
    // Our own uploads are fine too, and null removes the cover.
    let own = "/api/v1/files/01a1feab-397c-7512-88f7-a4520ff9ce09";
    let r = t
        .patch(&url, json!({ "cover_image": own }), Some(&ana))
        .await
        .body;
    assert_eq!(
        (r["cover_image"].as_str(), r["cover_credit"].is_null()),
        (Some(own), true)
    );
    let r = t
        .patch(&url, json!({ "cover_image": null }), Some(&ana))
        .await
        .body;
    assert!(r["cover_image"].is_null());
}
