mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Request, StatusCode, header};
use np_api::webhooks::{hex, hmac_sha256};
use serde_json::{Value, json};

async fn setup() -> (common::TestApp, String, String, String) {
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
    let con = t.join(&owner, "acme", "con@example.com", "member").await;
    let id = t.user_id(&con).await;
    t.post(
        &format!("{base}/members"),
        json!({ "user_id": id, "role": "contributor" }),
        Some(&owner),
    )
    .await;
    (t, owner, con, base)
}

#[tokio::test]
async fn public_board() {
    let (t, owner, con, base) = setup().await;
    let items = format!("{base}/work-items");
    t.post(
        &items,
        json!({ "title": "Shown", "description_html": "<p>Public text</p>" }),
        Some(&owner),
    )
    .await;
    t.post(
        &items,
        json!({ "title": "Draft", "is_draft": true }),
        Some(&owner),
    )
    .await;
    let old = t
        .post(&items, json!({ "title": "Old" }), Some(&owner))
        .await
        .body;
    t.complete(&format!("{items}/{}", old["id"].as_str().unwrap()), &owner)
        .await;
    t.post(
        &format!("{items}/{}/archive", old["id"].as_str().unwrap()),
        json!({}),
        Some(&owner),
    )
    .await;

    assert_eq!(
        t.post(
            &format!("{base}/publish"),
            json!({ "enabled": true }),
            Some(&con)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let on = t
        .post(
            &format!("{base}/publish"),
            json!({ "enabled": true }),
            Some(&owner),
        )
        .await
        .body;
    let path = on["path"].as_str().unwrap().to_string();
    let api = format!("/api/v1/public/boards/{}", path.trim_start_matches("/p/"));
    let b = t.get(&api, None).await;
    assert_eq!(b.status, StatusCode::OK, "{}", b.body);
    assert_eq!(b.body["project"]["identifier"], "WEB");
    let titles: Vec<&str> = b.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Shown"]);
    assert!(b.body["items"][0].get("description_html").is_none());
    assert!(b.body["states"].as_array().unwrap().len() >= 5);
    assert_eq!(
        t.get(&format!("{api}/items/1"), None).await.body["description_html"],
        "<p>Public text</p>"
    );
    assert_eq!(
        t.get(&format!("{api}/items/3"), None).await.status,
        StatusCode::NOT_FOUND,
        "archived items stay private"
    );
    t.post(
        &format!("{base}/publish"),
        json!({ "enabled": false }),
        Some(&owner),
    )
    .await;
    assert_eq!(t.get(&api, None).await.status, StatusCode::NOT_FOUND);
}

type Seen = Arc<Mutex<Vec<(HeaderMap, Bytes)>>>;

async fn receiver() -> (String, Seen) {
    let seen: Seen = Arc::default();
    let s = seen.clone();
    let app = axum::Router::new().route(
        "/hook",
        axum::routing::post(move |h: HeaderMap, b: Bytes| {
            let s = s.clone();
            async move {
                s.lock().unwrap().push((h, b));
                StatusCode::NO_CONTENT
            }
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (format!("http://{addr}/hook"), seen)
}

async fn wait_for(seen: &Seen, n: usize) -> Vec<(HeaderMap, Bytes)> {
    for _ in 0..50 {
        if seen.lock().unwrap().len() >= n {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    seen.lock().unwrap().clone()
}

#[tokio::test]
async fn webhooks_are_signed_filtered_and_logged() {
    let (t, owner, con, base) = setup().await;
    np_api::webhooks::spawn(
        t.store.clone(),
        t.events.clone(),
        reqwest::Client::new(),
        Some("https://plane.example.com".into()),
        "1.2.3".into(),
    );
    let (url, seen) = receiver().await;
    let hooks = "/api/v1/workspaces/acme/webhooks";
    assert_eq!(
        t.post(hooks, json!({ "url": url }), Some(&con))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.post(hooks, json!({ "url": "ftp://x" }), Some(&owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.post(
            hooks,
            json!({ "url": url, "events": ["nope"] }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let h = t
        .post(
            hooks,
            json!({ "url": url, "events": ["work_item"] }),
            Some(&owner),
        )
        .await;
    assert_eq!(h.status, StatusCode::CREATED, "{}", h.body);
    let secret = h.body["secret"].as_str().unwrap().to_string();
    let hid = h.body["id"].as_str().unwrap().to_string();
    assert!(
        t.get(hooks, Some(&owner)).await.body[0]
            .get("secret")
            .is_none(),
        "the secret is shown once"
    );
    // A second hook that cannot be reached.
    let dead = t
        .post(
            hooks,
            json!({ "url": "http://127.0.0.1:9/hook" }),
            Some(&owner),
        )
        .await
        .body;

    let item = t
        .post(
            &format!("{base}/work-items"),
            json!({ "title": "Ship it" }),
            Some(&con),
        )
        .await
        .body;
    let wid = item["id"].as_str().unwrap();
    t.post(
        &format!("{base}/work-items/{wid}/comments"),
        json!({ "body_html": "<p>hi</p>" }),
        Some(&con),
    )
    .await;
    t.patch(
        &format!("{base}/work-items/{wid}"),
        json!({ "priority": "high" }),
        Some(&con),
    )
    .await;

    let got = wait_for(&seen, 2).await;
    assert_eq!(
        got.len(),
        2,
        "two work item events, the comment is filtered out"
    );
    let (headers, body) = &got[0];
    let sig = headers["x-nextplane-signature"].to_str().unwrap();
    assert_eq!(
        sig,
        format!("sha256={}", hex(&hmac_sha256(secret.as_bytes(), body)))
    );
    assert_eq!(headers["x-nextplane-event"], "work_item.created");
    let v: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(v["work_item"]["key"], "WEB-1");
    assert_eq!(v["work_item"]["title"], "Ship it");
    assert!(
        v["work_item"]["url"]
            .as_str()
            .unwrap()
            .starts_with("https://plane.example.com/acme/projects/")
    );
    assert_eq!(
        v["delivery_id"],
        headers["x-nextplane-delivery"].to_str().unwrap()
    );
    let second: Value = serde_json::from_slice(&got[1].1).unwrap();
    assert_eq!(
        (
            second["event"].as_str(),
            second["work_item"]["priority"].as_str()
        ),
        (Some("work_item.updated"), Some("high"))
    );

    // Ping, logs and the failing hook's retry schedule.
    assert_eq!(
        t.post(&format!("{hooks}/{hid}/ping"), json!({}), Some(&owner))
            .await
            .status,
        StatusCode::ACCEPTED
    );
    assert_eq!(wait_for(&seen, 3).await.len(), 3);
    let log = t
        .get(&format!("{hooks}/{hid}/deliveries"), Some(&owner))
        .await
        .body;
    assert_eq!(log.as_array().unwrap().len(), 3);
    assert!(
        log.as_array()
            .unwrap()
            .iter()
            .all(|d| d["status"] == "delivered" && d["response_code"] == 204)
    );
    let dead_id = dead["id"].as_str().unwrap();
    let mut failed = Value::Null;
    for _ in 0..50 {
        failed = t
            .get(&format!("{hooks}/{dead_id}/deliveries"), Some(&owner))
            .await
            .body;
        if failed
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["attempts"] == 1)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let f = failed
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["attempts"] == 1)
        .unwrap_or_else(|| panic!("{failed}"));
    assert_eq!(
        (f["status"].as_str(), f["error"].as_str()),
        (Some("pending"), Some("could not connect"))
    );
    assert!(f["next_attempt_at"].is_string(), "retried later");

    // Rotate the secret; turn the hook off.
    let rotated = t
        .post(&format!("{hooks}/{hid}/rotate"), json!({}), Some(&owner))
        .await
        .body;
    assert_ne!(rotated["secret"], secret.as_str());
    assert_eq!(
        t.patch(
            &format!("{hooks}/{hid}"),
            json!({ "active": false }),
            Some(&owner)
        )
        .await
        .body["active"],
        false
    );
    assert_eq!(
        t.delete(&format!("{hooks}/{dead_id}"), Some(&owner))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
}

fn signed(path: &str, event: &str, secret: &str, body: &Value) -> Request<Body> {
    let bytes = body.to_string();
    Request::post(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-github-event", event)
        .header(
            "x-hub-signature-256",
            format!(
                "sha256={}",
                hex(&hmac_sha256(secret.as_bytes(), bytes.as_bytes()))
            ),
        )
        .body(Body::from(bytes))
        .unwrap()
}

#[tokio::test]
async fn github_links_and_closes() {
    let (t, owner, con, base) = setup().await;
    let a = t
        .post(
            &format!("{base}/work-items"),
            json!({ "title": "Login broken" }),
            Some(&owner),
        )
        .await
        .body;
    let b = t
        .post(
            &format!("{base}/work-items"),
            json!({ "title": "Docs" }),
            Some(&owner),
        )
        .await
        .body;
    let gh = "/api/v1/workspaces/acme/github";
    assert_eq!(
        t.post(gh, json!({}), Some(&con)).await.status,
        StatusCode::FORBIDDEN
    );
    let c = t.post(gh, json!({}), Some(&owner)).await.body;
    let (path, secret) = (
        c["path"].as_str().unwrap().to_string(),
        c["secret"].as_str().unwrap().to_string(),
    );

    let pr = json!({ "action": "closed", "pull_request": {
        "number": 7, "title": "Fix login", "body": "Fixes WEB-1. Also touches web-2.", "html_url": "https://github.com/acme/site/pull/7",
        "state": "closed", "merged": true, "head": { "ref": "web-1-login" } } });
    assert_eq!(
        t.send(signed(&path, "pull_request", "wrong", &pr))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.send(signed(
            "/api/v1/integrations/github/nope",
            "pull_request",
            &secret,
            &pr
        ))
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let r = t.send(signed(&path, "pull_request", &secret, &pr)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        (r.body["linked"].as_i64(), r.body["closed"].as_i64()),
        (Some(2), Some(1))
    );

    let ia = a["id"].as_str().unwrap();
    let links = t
        .get(&format!("{base}/work-items/{ia}/links"), Some(&owner))
        .await
        .body;
    assert_eq!(links[0]["title"], "PR #7: Fix login (merged)");
    let item = t
        .get(&format!("{base}/work-items/{ia}"), Some(&owner))
        .await
        .body;
    let states = t.get(&format!("{base}/states"), Some(&owner)).await.body;
    let group = states
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == item["state_id"])
        .unwrap()["group"]
        .clone();
    assert_eq!(group, "completed", "merged + \"Fixes\" completes the item");
    let ib = b["id"].as_str().unwrap();
    let item_b = t
        .get(&format!("{base}/work-items/{ib}"), Some(&owner))
        .await
        .body;
    assert_ne!(
        item_b["state_id"], item["state_id"],
        "a mention alone does not close"
    );

    // The same PR again only renames the link; a push links its commits.
    let again = t.send(signed(&path, "pull_request", &secret, &pr)).await;
    assert_eq!(again.body["closed"], 0);
    assert_eq!(
        t.get(&format!("{base}/work-items/{ia}/links"), Some(&owner))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let push = json!({ "commits": [{ "id": "abc1234def", "message": "WEB-2: write the docs\n\nmore", "url": "https://github.com/acme/site/commit/abc1234def" }] });
    assert_eq!(
        t.send(signed(&path, "push", &secret, &push)).await.body["linked"],
        1
    );
    let lb = t
        .get(&format!("{base}/work-items/{ib}/links"), Some(&owner))
        .await
        .body;
    assert!(
        lb.as_array()
            .unwrap()
            .iter()
            .any(|l| l["title"] == "Commit abc1234: WEB-2: write the docs"),
        "{lb}"
    );
    assert_eq!(
        t.send(signed(
            &path,
            "ping",
            &secret,
            &json!({ "zen": "Keep it simple" })
        ))
        .await
        .status,
        StatusCode::OK
    );
    assert!(t.get(gh, Some(&owner)).await.body["last_event_at"].is_string());

    // Turning it off makes the link unknown.
    t.delete(gh, Some(&owner)).await;
    assert_eq!(
        t.send(signed(&path, "push", &secret, &push)).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn visitors_who_sign_in_comment_react_and_vote_on_a_board() {
    let (t, owner, _con, base) = setup().await;
    let item = t
        .post(
            &format!("{base}/work-items"),
            json!({ "title": "Dark mode" }),
            Some(&owner),
        )
        .await
        .body;
    let wid = item["id"].as_str().unwrap().to_string();
    // An internal comment from the team.
    let internal = t
        .post(
            &format!("{base}/work-items/{wid}/comments"),
            json!({ "body_html": "<p>Team only</p>" }),
            Some(&owner),
        )
        .await
        .body;
    let p = t
        .post(
            &format!("{base}/publish"),
            json!({ "enabled": true }),
            Some(&owner),
        )
        .await
        .body;
    let path = p["path"].as_str().unwrap().to_string();
    let board = format!("/api/v1/public/boards/{}", path.trim_start_matches("/p/"));
    let one = format!("{board}/items/1");
    let r = t.get(&one, None).await.body;
    assert!(
        r.get("comments").is_none() && r.get("votes").is_none(),
        "off by default: {r}"
    );
    // Turning them on keeps the link.
    let s = t
        .post(
            &format!("{base}/publish"),
            json!({ "comments": true, "reactions": true, "votes": true }),
            Some(&owner),
        )
        .await
        .body;
    assert_eq!(
        (s["path"].as_str(), s["comments"].as_bool()),
        (Some(path.as_str()), Some(true))
    );
    assert_eq!(t.get(&board, None).await.body["allow"]["votes"], true);
    assert_eq!(
        t.get(&one, None).await.body["comments"],
        json!([]),
        "internal comments stay private"
    );

    // Reading is open; writing needs an account (any account on the server).
    let comment = json!({ "body_html": "<p>Please add this</p>" });
    assert_eq!(
        t.post(&format!("{one}/comments"), comment.clone(), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let vic = t.user("vic@example.com").await;
    let r = t
        .post(&format!("{one}/comments"), comment, Some(&vic))
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    let c = &r.body["comments"][0];
    assert_eq!(
        (c["author_name"].as_str(), c["mine"].as_bool()),
        (Some("vic"), Some(true))
    );
    let cid = c["id"].as_str().unwrap().to_string();
    // The team sees it in the app, marked public; the internal one can be made public too.
    let app = t
        .get(&format!("{base}/work-items/{wid}/comments"), Some(&owner))
        .await
        .body;
    assert!(
        app.as_array()
            .unwrap()
            .iter()
            .any(|x| x["id"] == cid.as_str() && x["is_public"] == true)
    );
    t.post(
        &format!(
            "{base}/work-items/{wid}/comments/{}/public",
            internal["id"].as_str().unwrap()
        ),
        json!({ "public": true }),
        Some(&owner),
    )
    .await;
    assert_eq!(
        t.get(&one, None).await.body["comments"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Reactions and one vote per person.
    let r = t
        .post(
            &format!("{one}/reactions"),
            json!({ "emoji": "👍" }),
            Some(&vic),
        )
        .await
        .body;
    assert_eq!(
        r["reactions"],
        json!([{ "emoji": "👍", "count": 1, "mine": true }])
    );
    let r = t
        .post(
            &format!("{one}/reactions"),
            json!({ "emoji": "🎉", "comment_id": cid }),
            Some(&vic),
        )
        .await
        .body;
    let mine = r["comments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == cid.as_str())
        .unwrap()
        .clone();
    assert_eq!(mine["reactions"][0]["emoji"], "🎉");
    let r = t
        .post(&format!("{one}/votes"), json!({ "value": 1 }), Some(&vic))
        .await
        .body;
    assert_eq!(r["votes"], json!({ "up": 1, "down": 0, "mine": 1 }));
    let r = t
        .post(
            &format!("{one}/votes"),
            json!({ "value": -1 }),
            Some(&owner),
        )
        .await
        .body;
    assert_eq!(r["votes"], json!({ "up": 1, "down": 1, "mine": -1 }));
    let r = t
        .post(&format!("{one}/votes"), json!({ "value": 0 }), Some(&vic))
        .await
        .body;
    assert_eq!(r["votes"]["up"], 0);
    assert_eq!(
        t.post(&format!("{one}/votes"), json!({ "value": 5 }), Some(&vic))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    // Only the author edits or deletes a comment; off means off.
    assert_eq!(
        t.patch(
            &format!("{one}/comments/{cid}"),
            json!({ "body_html": "<p>x</p>" }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let r = t
        .patch(
            &format!("{one}/comments/{cid}"),
            json!({ "body_html": "<p>Please add dark mode</p>" }),
            Some(&vic),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.body["comments"]
            .to_string()
            .contains("Please add dark mode")
    );
    assert_eq!(
        t.delete(&format!("{one}/comments/{cid}"), Some(&vic))
            .await
            .status,
        StatusCode::OK
    );
    t.post(
        &format!("{base}/publish"),
        json!({ "votes": false }),
        Some(&owner),
    )
    .await;
    assert_eq!(
        t.post(&format!("{one}/votes"), json!({ "value": 1 }), Some(&vic))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert!(t.get(&one, None).await.body.get("votes").is_none());
}
