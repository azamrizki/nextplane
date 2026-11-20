mod common;

use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn saved_views_visibility_and_ownership() {
    let t = common::app().await;
    let owner = t.setup_admin().await;
    t.workspace(&owner, "acme").await;
    let p = t
        .post(
            "/api/v1/workspaces/acme/projects",
            json!({ "name": "Website" }),
            Some(&owner),
        )
        .await
        .body;
    let base = format!(
        "/api/v1/workspaces/acme/projects/{}",
        p["id"].as_str().unwrap()
    );
    let a = t.join(&owner, "acme", "a@example.com", "member").await;
    let b = t.join(&owner, "acme", "b@example.com", "member").await;
    for x in [&a, &b] {
        t.post(&format!("{base}/join"), json!({}), Some(x)).await;
    }
    let v = format!("{base}/views");
    let cfg = json!({ "layout": "kanban", "filters": { "priority": ["urgent"] } });
    let mine = t
        .post(&v, json!({ "name": "My urgent", "config": cfg }), Some(&a))
        .await;
    assert_eq!(mine.status, StatusCode::CREATED, "{}", mine.body);
    let shared = t
        .post(
            &v,
            json!({ "name": "Team board", "is_shared": true, "config": cfg }),
            Some(&a),
        )
        .await
        .body;
    assert_eq!(
        t.post(&v, json!({ "name": "x", "config": [1] }), Some(&a))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    let names = |body: serde_json::Value| {
        body.as_array()
            .unwrap()
            .iter()
            .map(|x| x["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(t.get(&v, Some(&a)).await.body),
        ["My urgent", "Team board"]
    );
    assert_eq!(
        names(t.get(&v, Some(&b)).await.body),
        ["Team board"],
        "personal views stay personal"
    );

    let sid = shared["id"].as_str().unwrap();
    let mid = mine.body["id"].as_str().unwrap();
    assert_eq!(
        t.patch(&format!("{v}/{sid}"), json!({ "name": "Hijack" }), Some(&b))
            .await
            .status,
        StatusCode::FORBIDDEN,
        "contributors edit only their own"
    );
    assert_eq!(
        t.patch(&format!("{v}/{mid}"), json!({ "name": "x" }), Some(&b))
            .await
            .status,
        StatusCode::NOT_FOUND,
        "cannot even see it"
    );
    assert_eq!(
        t.patch(
            &format!("{v}/{sid}"),
            json!({ "name": "Team" }),
            Some(&owner)
        )
        .await
        .body["name"],
        "Team",
        "admins edit any"
    );
    assert_eq!(
        t.patch(
            &format!("{v}/{sid}"),
            json!({ "is_shared": false }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::FORBIDDEN,
        "only the owner unshares"
    );
    assert_eq!(
        t.delete(&format!("{v}/{sid}"), Some(&a)).await.status,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn my_work_spans_projects_and_respects_visibility() {
    let t = common::app().await;
    let owner = t.setup_admin().await;
    t.workspace(&owner, "acme").await;
    let ana_id = t.user_id(&owner).await;
    let mut bases = Vec::new();
    for (name, private) in [("Web", false), ("App", false), ("Secret", true)] {
        let p = t
            .post(
                "/api/v1/workspaces/acme/projects",
                json!({ "name": name, "is_private": private }),
                Some(&owner),
            )
            .await
            .body;
        bases.push(format!(
            "/api/v1/workspaces/acme/projects/{}",
            p["id"].as_str().unwrap()
        ));
    }
    t.post(
        &format!("{}/work-items", bases[0]),
        json!({ "title": "Web task", "assignee_ids": [ana_id] }),
        Some(&owner),
    )
    .await;
    t.post(
        &format!("{}/work-items", bases[1]),
        json!({ "title": "App task", "assignee_ids": [ana_id] }),
        Some(&owner),
    )
    .await;
    t.post(
        &format!("{}/work-items", bases[1]),
        json!({ "title": "Not mine" }),
        Some(&owner),
    )
    .await;
    let r = t
        .get("/api/v1/workspaces/acme/my-work", Some(&owner))
        .await
        .body;
    assert_eq!(r.as_array().unwrap().len(), 2);
    assert!(r[0]["identifier"].is_string());
    let created = t
        .get("/api/v1/workspaces/acme/my-work?kind=created", Some(&owner))
        .await
        .body;
    assert_eq!(created.as_array().unwrap().len(), 3);
    let subscribed = t
        .get(
            "/api/v1/workspaces/acme/my-work?kind=subscribed",
            Some(&owner),
        )
        .await
        .body;
    assert_eq!(
        subscribed.as_array().unwrap().len(),
        3,
        "creators follow their items"
    );
    assert_eq!(
        t.get("/api/v1/workspaces/acme/my-work?kind=x", Some(&owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    // A member who has joined nothing sees nothing, not even public projects' items.
    let m = t.join(&owner, "acme", "m@example.com", "member").await;
    assert!(
        t.get("/api/v1/workspaces/acme/my-work?kind=created", Some(&m))
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
}
