mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;

async fn project(t: &common::TestApp, owner: &str) -> String {
    t.workspace(owner, "acme").await;
    let p = t
        .post(
            "/api/v1/workspaces/acme/projects",
            json!({ "name": "Website" }),
            Some(owner),
        )
        .await
        .body;
    format!(
        "/api/v1/workspaces/acme/projects/{}",
        p["id"].as_str().unwrap()
    )
}

async fn bearer(
    t: &common::TestApp,
    method: &str,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> common::Res {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(if body.is_null() {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap();
    t.send(req).await
}

#[tokio::test]
async fn personal_tokens_read_and_write_scopes() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    let base = project(&t, &ana).await;

    let w = t
        .post(
            "/api/v1/me/tokens",
            json!({ "name": "CI", "scope": "write" }),
            Some(&ana),
        )
        .await;
    assert_eq!(w.status, StatusCode::CREATED);
    let write = w.body["token"].as_str().unwrap().to_string();
    assert!(write.starts_with("np_"));
    let r = t
        .post(
            "/api/v1/me/tokens",
            json!({ "name": "Reader", "scope": "read", "expires_in_days": 30 }),
            Some(&ana),
        )
        .await;
    let read = r.body["token"].as_str().unwrap().to_string();
    assert!(r.body["expires_at"].is_string());

    let created = bearer(
        &t,
        "POST",
        &format!("{base}/work-items"),
        &write,
        json!({ "title": "From CI" }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(
        bearer(
            &t,
            "GET",
            &format!("{base}/work-items"),
            &read,
            serde_json::Value::Null
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        bearer(
            &t,
            "POST",
            &format!("{base}/work-items"),
            &read,
            json!({ "title": "x" })
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        bearer(&t, "GET", "/api/v1/me", "np_nope", serde_json::Value::Null)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    let list = t.get("/api/v1/me/tokens", Some(&ana)).await.body;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert!(
        list[0].get("token").is_none() && list[0].get("token_hash").is_none(),
        "secrets are never listed"
    );
    let id = list
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "CI")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        t.delete(&format!("/api/v1/me/tokens/{id}"), Some(&ana))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        bearer(&t, "GET", "/api/v1/me", &write, serde_json::Value::Null)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn bots_work_as_workspace_members() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    let base = project(&t, &ana).await;
    let mem = t.join(&ana, "acme", "mem@example.com", "member").await;

    assert_eq!(
        t.post(
            "/api/v1/workspaces/acme/bots",
            json!({ "name": "Agent" }),
            Some(&mem)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let b = t
        .post(
            "/api/v1/workspaces/acme/bots",
            json!({ "name": "NextBoard agent" }),
            Some(&ana),
        )
        .await;
    assert_eq!(b.status, StatusCode::CREATED, "{}", b.body);
    let token = b.body["token"].as_str().unwrap().to_string();
    let bot_id = b.body["bot"]["id"].as_str().unwrap().to_string();
    assert_eq!(b.body["bot"]["is_bot"], true);

    // Bot is a workspace member: it can join the public project and work there.
    assert_eq!(
        bearer(&t, "POST", &format!("{base}/join"), &token, json!({}))
            .await
            .body["my_role"],
        "contributor"
    );
    let it = bearer(
        &t,
        "POST",
        &format!("{base}/work-items"),
        &token,
        json!({ "title": "Agent task" }),
    )
    .await;
    assert_eq!(it.body["created_by"], bot_id);
    // Bots cannot log in with a password or make more tokens.
    assert_eq!(
        bearer(
            &t,
            "POST",
            "/api/v1/me/tokens",
            &token,
            json!({ "name": "x" })
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );

    let bots = t.get("/api/v1/workspaces/acme/bots", Some(&ana)).await.body;
    assert_eq!(bots[0]["name"], "NextBoard agent");
    assert_eq!(bots[0]["tokens"].as_array().unwrap().len(), 1);
    let rotated = t
        .post(
            &format!("/api/v1/workspaces/acme/bots/{bot_id}/token"),
            json!({}),
            Some(&ana),
        )
        .await
        .body["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        bearer(&t, "GET", "/api/v1/me", &token, serde_json::Value::Null)
            .await
            .status,
        StatusCode::UNAUTHORIZED,
        "old token revoked"
    );
    assert_eq!(
        bearer(&t, "GET", "/api/v1/me", &rotated, serde_json::Value::Null)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        t.delete(
            &format!("/api/v1/workspaces/acme/bots/{bot_id}"),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        bearer(&t, "GET", "/api/v1/me", &rotated, serde_json::Value::Null)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn search_finds_titles_descriptions_comments_and_keys() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    let base = project(&t, &ana).await;
    let a = t.post(&format!("{base}/work-items"), json!({ "title": "Login redirect broken", "description_html": "<p>Users see a <b>blank</b> page</p>" }), Some(&ana)).await.body;
    let b = t
        .post(
            &format!("{base}/work-items"),
            json!({ "title": "Release notes" }),
            Some(&ana),
        )
        .await
        .body;
    t.post(
        &format!("{base}/work-items/{}/comments", b["id"].as_str().unwrap()),
        json!({ "body_html": "<p>mention the changelog</p>" }),
        Some(&ana),
    )
    .await;
    let private = t
        .post(
            "/api/v1/workspaces/acme/projects",
            json!({ "name": "Secret", "is_private": true }),
            Some(&ana),
        )
        .await
        .body;
    t.post(
        &format!(
            "/api/v1/workspaces/acme/projects/{}/work-items",
            private["id"].as_str().unwrap()
        ),
        json!({ "title": "Secret login plan" }),
        Some(&ana),
    )
    .await;

    let s = |q: &str, who: &str| {
        let path = format!("/api/v1/workspaces/acme/search?q={q}");
        let who = who.to_string();
        let t = &t;
        async move { t.get(&path, Some(&who)).await.body }
    };
    let r = s("logi", &ana).await;
    assert_eq!(
        r["work_items"].as_array().unwrap().len(),
        2,
        "prefix match, owner sees the private project"
    );
    assert_eq!(
        s("blank", &ana).await["work_items"][0]["work_item_id"],
        a["id"]
    );
    assert_eq!(
        s("changelog", &ana).await["work_items"][0]["kind"],
        "comment"
    );
    assert_eq!(
        s("WEB-2", &ana).await["work_items"][0]["title"],
        "Release notes"
    );
    assert_eq!(s("secret", &ana).await["projects"][0]["name"], "Secret");
    assert!(
        s("%22%20OR%20%2A", &ana).await["work_items"]
            .as_array()
            .unwrap()
            .is_empty(),
        "odd input is safe"
    );

    let mem = t.join(&ana, "acme", "mem@example.com", "member").await;
    t.post(&format!("{base}/join"), json!({}), Some(&mem)).await;
    let r = s("login", &mem).await;
    assert_eq!(
        r["work_items"].as_array().unwrap().len(),
        1,
        "the private project stays hidden"
    );

    // Renames and deletes keep the index current.
    let aid = a["id"].as_str().unwrap();
    t.patch(
        &format!("{base}/work-items/{aid}"),
        json!({ "title": "Auth redirect" }),
        Some(&ana),
    )
    .await;
    assert_eq!(
        s("auth", &ana).await["work_items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    t.delete(&format!("{base}/work-items/{aid}"), Some(&ana))
        .await;
    assert!(
        s("auth", &ana).await["work_items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn pagination_and_conflicts() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    let base = project(&t, &ana).await;
    for i in 0..7 {
        t.post(
            &format!("{base}/work-items"),
            json!({ "title": format!("Item {i}") }),
            Some(&ana),
        )
        .await;
    }
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let path = match &cursor {
            Some(c) => format!("{base}/work-items?limit=3&cursor={c}"),
            None => format!("{base}/work-items?limit=3"),
        };
        let req = Request::get(&path)
            .header(header::COOKIE, &ana)
            .body(Body::empty())
            .unwrap();
        let res = tower::ServiceExt::oneshot(t.app.clone(), req)
            .await
            .unwrap();
        cursor = res
            .headers()
            .get("x-next-cursor")
            .map(|v| v.to_str().unwrap().to_string());
        let bytes = http_body_util::BodyExt::collect(res.into_body())
            .await
            .unwrap()
            .to_bytes();
        let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        seen.extend(
            page.as_array()
                .unwrap()
                .iter()
                .map(|i| i["sequence"].as_i64().unwrap()),
        );
        if cursor.is_none() {
            break;
        }
    }
    seen.sort();
    assert_eq!(seen, (1..=7).collect::<Vec<_>>(), "every item once");
    assert_eq!(
        t.get(&format!("{base}/work-items?cursor=zzz"), Some(&ana))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    let it = t
        .get(&format!("{base}/work-items?limit=1"), Some(&ana))
        .await
        .body[0]
        .clone();
    let id = it["id"].as_str().unwrap();
    let stamp = it["updated_at"].as_str().unwrap();
    let url = format!("{base}/work-items/{id}");
    // My own saves in a row never conflict.
    assert_eq!(
        t.patch(
            &url,
            json!({ "title": "Mine", "expected_updated_at": stamp }),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        t.patch(
            &url,
            json!({ "description_html": "<p>a</p>", "expected_updated_at": stamp }),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::OK
    );
    // Someone else's property change does not conflict with my text; their text change does.
    let bob = t.join(&ana, "acme", "bob@example.com", "member").await;
    t.post(&format!("{base}/join"), json!({}), Some(&bob)).await;
    assert_eq!(
        t.patch(&url, json!({ "priority": "high" }), Some(&bob))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        t.patch(
            &url,
            json!({ "title": "Mine 2", "expected_updated_at": stamp }),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        t.patch(
            &url,
            json!({ "title": "Bob's", "expected_updated_at": stamp }),
            Some(&bob)
        )
        .await
        .status,
        StatusCode::CONFLICT,
        "ana changed the title after bob's version"
    );
    let fresh = t.get(&url, Some(&bob)).await.body["updated_at"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        t.patch(
            &url,
            json!({ "title": "Bob's", "expected_updated_at": fresh }),
            Some(&bob)
        )
        .await
        .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn changes_are_published_as_events() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    let mut rx = t.events.subscribe();
    let base = project(&t, &ana).await;
    let mut areas = Vec::new();
    while let Ok(e) = rx.try_recv() {
        areas.push((e.workspace.clone(), e.area.clone(), e.project_id.is_some()));
    }
    // Creating the workspace itself happens outside any workspace channel; the project does not.
    assert!(
        areas.contains(&("acme".into(), "project".into(), false)),
        "{areas:?}"
    );

    let it = t
        .post(
            &format!("{base}/work-items"),
            json!({ "title": "x" }),
            Some(&ana),
        )
        .await
        .body;
    let e = rx.try_recv().unwrap();
    assert_eq!(e.area, "work_item");
    assert!(e.project_id.is_some());
    assert!(e.actor.is_some());
    t.post(
        &format!("{base}/work-items/{}/comments", it["id"].as_str().unwrap()),
        json!({ "body_html": "<p>hi</p>" }),
        Some(&ana),
    )
    .await;
    let e = rx.try_recv().unwrap();
    assert_eq!(e.area, "comment");
    assert_eq!(
        e.work_item_id.unwrap().to_string(),
        it["id"].as_str().unwrap()
    );
    // Reads and failures publish nothing.
    t.get(&format!("{base}/work-items"), Some(&ana)).await;
    t.post(
        &format!("{base}/work-items"),
        json!({ "title": "" }),
        Some(&ana),
    )
    .await;
    assert!(rx.try_recv().is_err());
}
