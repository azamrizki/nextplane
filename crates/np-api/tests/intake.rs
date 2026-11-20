mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

struct W {
    t: common::TestApp,
    owner: String,
    con: String,
    com: String,
    guest: String,
    base: String,
}

async fn world() -> W {
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
    t.patch(&base, json!({ "feature_intake": true }), Some(&owner))
        .await;
    let mut people = Vec::new();
    for (email, ws_role, role) in [
        ("con@example.com", "member", "contributor"),
        ("com@example.com", "member", "commenter"),
        ("guest@example.com", "guest", "guest"),
    ] {
        let c = t.join(&owner, "acme", email, ws_role).await;
        let id = t.user_id(&c).await;
        let r = t
            .post(
                &format!("{base}/members"),
                json!({ "user_id": id, "role": role }),
                Some(&owner),
            )
            .await;
        assert!(r.status.is_success(), "{}", r.body);
        people.push(c);
    }
    let guest = people.pop().unwrap();
    let com = people.pop().unwrap();
    let con = people.pop().unwrap();
    W {
        t,
        owner,
        con,
        com,
        guest,
        base,
    }
}

fn titles(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn submit_see_own_and_triage() {
    let w = world().await;
    let ib = format!("{}/intake", w.base);
    let g = w.t.post(&ib, json!({ "title": "Export to PDF please", "description_html": "<p>For reports<script>x</script></p>" }), Some(&w.guest)).await;
    assert_eq!(g.status, StatusCode::CREATED, "{}", g.body);
    assert_eq!(g.body["status"], "pending");
    let gid = g.body["work_item_id"].as_str().unwrap().to_string();
    let c =
        w.t.post(
            &ib,
            json!({ "title": "Dark mode is too dark" }),
            Some(&w.com),
        )
        .await
        .body;
    let cid = c["work_item_id"].as_str().unwrap().to_string();

    // Guests see only their own; commenters see everything.
    assert_eq!(
        titles(&w.t.get(&ib, Some(&w.guest)).await.body),
        ["Export to PDF please"]
    );
    assert_eq!(
        w.t.get(&format!("{ib}/{cid}"), Some(&w.guest)).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(titles(&w.t.get(&ib, Some(&w.com)).await.body).len(), 2);
    assert_eq!(
        w.t.get(&format!("{ib}/count"), Some(&w.owner)).await.body["open"],
        2
    );
    let desc = w.t.get(&format!("{ib}/{gid}"), Some(&w.guest)).await.body;
    assert!(
        !desc["description_html"]
            .as_str()
            .unwrap()
            .contains("script")
    );

    // Not part of the project yet: not listed, not found by key, not searchable.
    assert!(
        w.t.get(&format!("{}/work-items", w.base), Some(&w.owner))
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        w.t.get(
            "/api/v1/workspaces/acme/work-items/by-key/WEB-1",
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert!(
        w.t.get("/api/v1/workspaces/acme/search?q=PDF", Some(&w.owner))
            .await
            .body["work_items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // The admin heard about it.
    assert!(
        w.t.get(
            "/api/v1/workspaces/acme/notifications/count",
            Some(&w.owner)
        )
        .await
        .body["unread"]
            .as_i64()
            .unwrap()
            >= 2
    );

    // Submitters edit their own while pending; others and the regular API cannot.
    assert_eq!(
        w.t.patch(
            &format!("{ib}/{gid}"),
            json!({ "title": "Export to PDF" }),
            Some(&w.guest)
        )
        .await
        .body["title"],
        "Export to PDF"
    );
    assert_eq!(
        w.t.patch(
            &format!("{ib}/{gid}"),
            json!({ "title": "x" }),
            Some(&w.com)
        )
        .await
        .status,
        StatusCode::FORBIDDEN,
        "others' submissions"
    );
    assert_eq!(
        w.t.patch(
            &format!("{}/work-items/{gid}", w.base),
            json!({ "title": "x" }),
            Some(&w.con)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );

    // Only admins triage.
    assert_eq!(
        w.t.post(&format!("{ib}/{gid}/accept"), json!({}), Some(&w.con))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let d =
        w.t.post(
            &format!("{ib}/{cid}/decline"),
            json!({ "reason": "Works as designed" }),
            Some(&w.owner),
        )
        .await;
    assert_eq!(
        (d.body["status"].as_str(), d.body["reason"].as_str()),
        (Some("declined"), Some("Works as designed")),
        "{}",
        d.body
    );
    assert_eq!(
        w.t.patch(
            &format!("{ib}/{cid}"),
            json!({ "title": "x" }),
            Some(&w.com)
        )
        .await
        .status,
        StatusCode::FORBIDDEN,
        "closed for the submitter"
    );
    let notes =
        w.t.get("/api/v1/workspaces/acme/notifications", Some(&w.com))
            .await
            .body;
    assert!(
        notes
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["field"] == "intake" && n["new_value"] == "declined"),
        "{notes}"
    );
    assert_eq!(
        w.t.post(&format!("{ib}/{cid}/reopen"), Value::Null, Some(&w.owner))
            .await
            .body["status"],
        "pending"
    );

    // Accept: the item joins the project.
    let a =
        w.t.post(&format!("{ib}/{gid}/accept"), Value::Null, Some(&w.owner))
            .await;
    assert_eq!(a.body["status"], "accepted", "{}", a.body);
    assert_eq!(
        titles(
            &w.t.get(&format!("{}/work-items", w.base), Some(&w.con))
                .await
                .body
        ),
        ["Export to PDF"]
    );
    assert_eq!(
        w.t.get(
            "/api/v1/workspaces/acme/work-items/by-key/WEB-1",
            Some(&w.com)
        )
        .await
        .body["title"],
        "Export to PDF"
    );
    assert_eq!(
        w.t.get("/api/v1/workspaces/acme/search?q=PDF", Some(&w.com))
            .await
            .body["work_items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        w.t.post(&format!("{ib}/{gid}/decline"), json!({}), Some(&w.owner))
            .await
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        titles(
            &w.t.get(&format!("{ib}?view=closed"), Some(&w.guest))
                .await
                .body
        ),
        ["Export to PDF"],
        "the guest still sees the outcome"
    );

    // Duplicate of an existing item, by key.
    let dup =
        w.t.post(
            &format!("{ib}/{cid}/duplicate"),
            json!({ "duplicate_of": "web-1" }),
            Some(&w.owner),
        )
        .await;
    assert_eq!(
        (
            dup.body["status"].as_str(),
            dup.body["duplicate_sequence"].as_i64()
        ),
        (Some("duplicate"), Some(1)),
        "{}",
        dup.body
    );
    assert_eq!(
        w.t.post(
            &format!("{ib}/{cid}/duplicate"),
            json!({ "duplicate_of": "WEB-99" }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    // Snooze hides it from the open queue until the date.
    let s =
        w.t.post(&ib, json!({ "title": "Later" }), Some(&w.con))
            .await
            .body;
    let sid = s["work_item_id"].as_str().unwrap();
    assert_eq!(
        w.t.post(
            &format!("{ib}/{sid}/snooze"),
            json!({ "until": "2001-01-01" }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        w.t.post(
            &format!("{ib}/{sid}/snooze"),
            json!({ "until": "2999-01-01" }),
            Some(&w.owner)
        )
        .await
        .body["status"],
        "snoozed"
    );
    assert!(!titles(&w.t.get(&ib, Some(&w.owner)).await.body).contains(&"Later".to_string()));
    assert_eq!(
        titles(
            &w.t.get(&format!("{ib}?view=snoozed"), Some(&w.owner))
                .await
                .body
        ),
        ["Later"]
    );

    // The submitter withdraws a pending one.
    let x =
        w.t.post(&ib, json!({ "title": "Oops" }), Some(&w.guest))
            .await
            .body;
    assert_eq!(
        w.t.delete(
            &format!("{ib}/{}", x["work_item_id"].as_str().unwrap()),
            Some(&w.guest)
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );

    // Turned off: no intake at all.
    w.t.patch(&w.base, json!({ "feature_intake": false }), Some(&w.owner))
        .await;
    assert_eq!(
        w.t.get(&ib, Some(&w.owner)).await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn public_form() {
    let w = world().await;
    let ib = format!("{}/intake", w.base);
    assert_eq!(
        w.t.post(
            &format!("{}/intake-form", w.base),
            json!({ "enabled": true }),
            Some(&w.con)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let on =
        w.t.post(
            &format!("{}/intake-form", w.base),
            json!({ "enabled": true }),
            Some(&w.owner),
        )
        .await
        .body;
    let path = on["path"].as_str().unwrap().to_string();
    let api = format!("/api/v1/public{path}");
    let info = w.t.get(&api, None).await;
    assert_eq!(
        (
            info.body["workspace"].as_str(),
            info.body["project"].as_str()
        ),
        (Some("ACME"), Some("Website")),
        "{}",
        info.body
    );

    // A bot filling the hidden field gets a polite answer and nothing is created.
    assert_eq!(
        w.t.post(
            &api,
            json!({ "title": "Buy pills", "website": "spam.example" }),
            None
        )
        .await
        .status,
        StatusCode::ACCEPTED
    );
    let sent = w.t.post(&api, json!({ "name": "Rina", "email": "Rina@Example.com", "title": "Cannot reset my password", "description": "Link <b>expired</b>" }), None).await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.body);
    let q = w.t.get(&ib, Some(&w.owner)).await.body;
    assert_eq!(titles(&q), ["Cannot reset my password"]);
    assert_eq!(
        (q[0]["source"].as_str(), q[0]["submitter_email"].as_str()),
        (Some("form"), Some("rina@example.com"))
    );
    assert!(
        w.t.get(&ib, Some(&w.com)).await.body[0]["submitter_email"].is_null(),
        "emails are for admins"
    );
    let id = q[0]["work_item_id"].as_str().unwrap();
    assert_eq!(
        w.t.get(&format!("{ib}/{id}"), Some(&w.owner)).await.body["description_html"],
        "<p>Link &lt;b&gt;expired&lt;/b&gt;</p>"
    );
    assert_eq!(
        w.t.post(&api, json!({ "title": "" }), None).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        w.t.post(&api, json!({ "title": "x", "email": "not-an-email" }), None)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    // Five tries per address and ten minutes (four so far).
    assert_eq!(
        w.t.post(&api, json!({ "title": "Fifth" }), None)
            .await
            .status,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        w.t.post(&api, json!({ "title": "Sixth" }), None)
            .await
            .status,
        StatusCode::TOO_MANY_REQUESTS
    );

    // A new link replaces the old one; off means gone.
    let again =
        w.t.post(
            &format!("{}/intake-form", w.base),
            json!({ "enabled": true }),
            Some(&w.owner),
        )
        .await
        .body;
    assert_ne!(again["path"], on["path"]);
    assert_eq!(w.t.get(&api, None).await.status, StatusCode::NOT_FOUND);
    w.t.post(
        &format!("{}/intake-form", w.base),
        json!({ "enabled": false }),
        Some(&w.owner),
    )
    .await;
    assert_eq!(
        w.t.get(
            &format!("/api/v1/public{}", again["path"].as_str().unwrap()),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}
