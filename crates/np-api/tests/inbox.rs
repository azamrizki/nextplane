mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

struct W {
    t: common::TestApp,
    ana: String,
    bob: String,
    cid: String,
    base: String,
}

async fn world() -> W {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    t.workspace(&ana, "acme").await;
    let p = t
        .post(
            "/api/v1/workspaces/acme/projects",
            json!({ "name": "Website" }),
            Some(&ana),
        )
        .await
        .body;
    let base = format!(
        "/api/v1/workspaces/acme/projects/{}",
        p["id"].as_str().unwrap()
    );
    let bob = t.join(&ana, "acme", "bob@example.com", "member").await;
    t.post(&format!("{base}/join"), json!({}), Some(&bob)).await;
    let cid = t.user_id(&bob).await;
    W {
        t,
        ana,
        bob,
        cid,
        base,
    }
}

const INBOX: &str = "/api/v1/workspaces/acme/notifications";

async fn inbox(w: &W, who: &str, filter: &str) -> Vec<Value> {
    w.t.get(&format!("{INBOX}?filter={filter}"), Some(who))
        .await
        .body
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn assignment_changes_comments_and_mentions_notify_the_right_people() {
    let w = world().await;
    let bob_id = w.cid.clone();
    let it =
        w.t.post(
            &format!("{}/work-items", w.base),
            json!({ "title": "Fix login", "assignee_ids": [bob_id] }),
            Some(&w.ana),
        )
        .await
        .body;
    let id = it["id"].as_str().unwrap().to_string();
    let url = format!("{}/work-items/{id}", w.base);

    let b = inbox(&w, &w.bob, "unread").await;
    assert_eq!(b.len(), 1);
    assert_eq!(b[0]["kind"], "assigned");
    assert_eq!(b[0]["identifier"], "WEB");
    assert!(
        inbox(&w, &w.ana, "unread").await.is_empty(),
        "no notification for your own change"
    );

    // Ana changes the priority: Bob (assignee, subscribed) hears about it.
    w.t.patch(&url, json!({ "priority": "urgent" }), Some(&w.ana))
        .await;
    let b = inbox(&w, &w.bob, "unread").await;
    assert_eq!(b[0]["kind"], "updated");
    assert_eq!(b[0]["field"], "priority");
    assert_eq!(b[0]["new_value"], "urgent");

    // Bob comments: Ana (creator, subscribed) gets "commented".
    let html = format!(
        r#"<p>done, <span data-type="mention" data-id="{}">@Ana</span></p>"#,
        w.t.user_id(&w.ana).await
    );
    w.t.post(
        &format!("{url}/comments"),
        json!({ "body_html": "<p>on it</p>" }),
        Some(&w.bob),
    )
    .await;
    assert_eq!(inbox(&w, &w.ana, "unread").await[0]["kind"], "commented");
    w.t.post(
        &format!("{url}/comments"),
        json!({ "body_html": html }),
        Some(&w.bob),
    )
    .await;
    let a = inbox(&w, &w.ana, "unread").await;
    assert_eq!(
        a[0]["kind"], "mentioned",
        "a mention replaces the plain comment notice"
    );
    assert_eq!(a.len(), 2);

    // Mentions in a description reach people who do not follow the item.
    let cara = w.t.join(&w.ana, "acme", "cara@example.com", "member").await;
    w.t.post(&format!("{}/join", w.base), json!({}), Some(&cara))
        .await;
    let cara_id = w.t.user_id(&cara).await;
    let desc = format!(r#"<p>ask <span data-type="mention" data-id="{cara_id}">@Cara</span></p>"#);
    w.t.patch(&url, json!({ "description_html": desc }), Some(&w.ana))
        .await;
    let c = inbox(&w, &cara, "unread").await;
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["kind"], "mentioned");
    // Saving the description again does not mention her twice (she now follows the item, so
    // she does hear that the description changed).
    w.t.patch(
        &url,
        json!({ "description_html": format!("{desc}<p>more</p>") }),
        Some(&w.ana),
    )
    .await;
    let c = inbox(&w, &cara, "unread").await;
    assert_eq!(c.iter().filter(|n| n["kind"] == "mentioned").count(), 1);
    assert_eq!(c[0]["kind"], "updated");
    assert_eq!(c[0]["field"], "description");
}

#[tokio::test]
async fn read_archive_snooze_and_counts() {
    let w = world().await;
    for i in 0..3 {
        w.t.post(
            &format!("{}/work-items", w.base),
            json!({ "title": format!("T{i}"), "assignee_ids": [w.cid] }),
            Some(&w.ana),
        )
        .await;
    }
    async fn count(w: &W) -> i64 {
        w.t.get(&format!("{INBOX}/count"), Some(&w.bob)).await.body["unread"]
            .as_i64()
            .unwrap()
    }
    assert_eq!(count(&w).await, 3);
    let list = inbox(&w, &w.bob, "unread").await;
    let (a, b, c) = (
        list[0]["id"].as_str().unwrap(),
        list[1]["id"].as_str().unwrap(),
        list[2]["id"].as_str().unwrap(),
    );
    assert_eq!(
        w.t.post(&format!("{INBOX}/{a}/read"), Value::Null, Some(&w.bob))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(count(&w).await, 2);
    w.t.post(&format!("{INBOX}/{b}/archive"), Value::Null, Some(&w.bob))
        .await;
    assert_eq!(inbox(&w, &w.bob, "archived").await.len(), 1);
    assert_eq!(inbox(&w, &w.bob, "all").await.len(), 2);
    let snooze =
        w.t.post(
            &format!("{INBOX}/{c}/snooze"),
            json!({ "until": "2999-01-01" }),
            Some(&w.bob),
        )
        .await;
    assert_eq!(snooze.status, StatusCode::NO_CONTENT);
    assert_eq!(inbox(&w, &w.bob, "snoozed").await.len(), 1);
    assert_eq!(count(&w).await, 0);
    assert_eq!(
        w.t.post(
            &format!("{INBOX}/{c}/snooze"),
            json!({ "until": "2000-01-01" }),
            Some(&w.bob)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        w.t.post(&format!("{INBOX}/{c}/explode"), Value::Null, Some(&w.bob))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    // Not your notification: 404.
    assert_eq!(
        w.t.post(&format!("{INBOX}/{a}/unread"), Value::Null, Some(&w.ana))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    w.t.post(&format!("{INBOX}/{a}/unread"), Value::Null, Some(&w.bob))
        .await;
    assert_eq!(
        w.t.post(&format!("{INBOX}/read-all"), json!({}), Some(&w.bob))
            .await
            .body["marked"],
        1
    );

    let p =
        w.t.patch(
            "/api/v1/me/preferences",
            json!({ "email_notifications": false }),
            Some(&w.bob),
        )
        .await;
    assert_eq!(p.body["email_notifications"], false);
}

#[tokio::test]
async fn people_who_lose_access_get_nothing() {
    let w = world().await;
    let it =
        w.t.post(
            &format!("{}/work-items", w.base),
            json!({ "title": "x", "assignee_ids": [w.cid] }),
            Some(&w.ana),
        )
        .await
        .body;
    w.t.post(&format!("{}/leave", w.base), json!({}), Some(&w.bob))
        .await;
    w.t.patch(
        &format!("{}/work-items/{}", w.base, it["id"].as_str().unwrap()),
        json!({ "priority": "low" }),
        Some(&w.ana),
    )
    .await;
    assert_eq!(
        inbox(&w, &w.bob, "unread").await.len(),
        1,
        "only the earlier assignment"
    );
}

#[tokio::test]
async fn drafts_are_private_until_published() {
    let w = world().await;
    let d =
        w.t.post(
            &format!("{}/work-items", w.base),
            json!({ "title": "Half an idea", "is_draft": true, "assignee_ids": [w.cid] }),
            Some(&w.ana),
        )
        .await
        .body;
    let id = d["id"].as_str().unwrap();
    assert_eq!(d["is_draft"], true);
    assert!(
        w.t.get(&format!("{}/work-items", w.base), Some(&w.ana))
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        w.t.get(&format!("{}/work-items/{id}", w.base), Some(&w.bob))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert!(
        inbox(&w, &w.bob, "unread").await.is_empty(),
        "no notifications for drafts"
    );
    assert!(
        w.t.get("/api/v1/workspaces/acme/search?q=idea", Some(&w.ana))
            .await
            .body["work_items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mine =
        w.t.get("/api/v1/workspaces/acme/drafts", Some(&w.ana))
            .await
            .body;
    assert_eq!(mine[0]["title"], "Half an idea");
    assert!(
        w.t.get("/api/v1/workspaces/acme/drafts", Some(&w.bob))
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );

    assert_eq!(
        w.t.post(
            &format!("{}/work-items/{id}/publish", w.base),
            json!({}),
            Some(&w.bob)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let p =
        w.t.post(
            &format!("{}/work-items/{id}/publish", w.base),
            json!({}),
            Some(&w.ana),
        )
        .await;
    assert_eq!(p.body["is_draft"], false);
    assert_eq!(
        w.t.get(&format!("{}/work-items", w.base), Some(&w.bob))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(inbox(&w, &w.bob, "unread").await[0]["kind"], "assigned");
    assert_eq!(
        w.t.get("/api/v1/workspaces/acme/search?q=idea", Some(&w.ana))
            .await
            .body["work_items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn digest_batches_respect_preferences_and_send_once() {
    let w = world().await;
    w.t.post(
        &format!("{}/work-items", w.base),
        json!({ "title": "x", "assignee_ids": [w.cid] }),
        Some(&w.ana),
    )
    .await;
    let later = "2999-01-01T00:00:00.000000Z";
    let b = w.t.store.digest_batches(later).await.unwrap();
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "bob@example.com");
    assert_eq!(b[0].2.len(), 1);
    assert!(
        w.t.store
            .digest_batches("2000-01-01T00:00:00.000000Z")
            .await
            .unwrap()
            .is_empty(),
        "too recent"
    );
    let ids: Vec<_> = b[0].2.iter().map(|n| n.id).collect();
    w.t.store.mark_emailed(&ids).await.unwrap();
    assert!(
        w.t.store.digest_batches(later).await.unwrap().is_empty(),
        "never twice"
    );

    w.t.patch(
        "/api/v1/me/preferences",
        json!({ "email_notifications": false }),
        Some(&w.bob),
    )
    .await;
    w.t.post(
        &format!("{}/work-items", w.base),
        json!({ "title": "y", "assignee_ids": [w.cid] }),
        Some(&w.ana),
    )
    .await;
    assert!(
        w.t.store.digest_batches(later).await.unwrap().is_empty(),
        "email turned off"
    );
}
