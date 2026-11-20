mod common;

use axum::http::StatusCode;
use serde_json::json;

struct W {
    t: common::TestApp,
    owner: String,
    contrib: String,
    commenter: String,
    outsider: String,
    item: String,
    base: String,
    pbase: String,
}

async fn world() -> W {
    let t = common::app().await;
    let owner = t.setup_admin().await;
    t.workspace(&owner, "acme").await;
    let contrib = t.join(&owner, "acme", "con@example.com", "member").await;
    let commenter = t.join(&owner, "acme", "com@example.com", "member").await;
    let outsider = t.join(&owner, "acme", "out@example.com", "member").await;
    let p = t
        .post(
            "/api/v1/workspaces/acme/projects",
            json!({ "name": "Website", "is_private": true }),
            Some(&owner),
        )
        .await
        .body;
    let pbase = format!(
        "/api/v1/workspaces/acme/projects/{}",
        p["id"].as_str().unwrap()
    );
    for (c, role) in [(&contrib, "contributor"), (&commenter, "commenter")] {
        let id = t.user_id(c).await;
        t.post(
            &format!("{pbase}/members"),
            json!({ "user_id": id, "role": role }),
            Some(&owner),
        )
        .await;
    }
    let it = t
        .post(
            &format!("{pbase}/work-items"),
            json!({ "title": "Task" }),
            Some(&owner),
        )
        .await
        .body;
    let item = it["id"].as_str().unwrap().to_string();
    let base = format!("{pbase}/work-items/{item}");
    W {
        t,
        owner,
        contrib,
        commenter,
        outsider,
        item,
        base,
        pbase,
    }
}

#[tokio::test]
async fn comments_follow_author_only_rules() {
    let w = world().await;
    let c = format!("{}/comments", w.base);
    let r =
        w.t.post(
            &c,
            json!({ "body_html": "<p>From commenter<script>x</script></p>" }),
            Some(&w.commenter),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    assert!(!r.body["body_html"].as_str().unwrap().contains("script"));
    let cid = r.body["id"].as_str().unwrap().to_string();
    assert_eq!(
        w.t.post(&c, json!({ "body_html": "<p>  </p>" }), Some(&w.owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    // Only the author edits or deletes, admins included.
    assert_eq!(
        w.t.patch(
            &format!("{c}/{cid}"),
            json!({ "body_html": "<p>x</p>" }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.t.delete(&format!("{c}/{cid}"), Some(&w.owner))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let e =
        w.t.patch(
            &format!("{c}/{cid}"),
            json!({ "body_html": "<p>Edited</p>" }),
            Some(&w.commenter),
        )
        .await;
    assert!(e.body["edited_at"].is_string());

    // Admin hides; the text is no longer sent; a contributor cannot hide.
    assert_eq!(
        w.t.post(&format!("{c}/{cid}/hide"), json!({}), Some(&w.contrib))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let h =
        w.t.post(&format!("{c}/{cid}/hide"), json!({}), Some(&w.owner))
            .await;
    assert_eq!(h.body["body_html"], "");
    assert!(h.body["hidden_at"].is_string());
    w.t.post(&format!("{c}/{cid}/unhide"), json!({}), Some(&w.owner))
        .await;
    assert_eq!(
        w.t.get(&c, Some(&w.contrib)).await.body[0]["body_html"],
        "<p>Edited</p>"
    );

    assert_eq!(
        w.t.delete(&format!("{c}/{cid}"), Some(&w.commenter))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert!(
        w.t.get(&c, Some(&w.owner))
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Not in the project: 404, not even the list.
    assert_eq!(
        w.t.get(&c, Some(&w.outsider)).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn mentions_and_comments_subscribe_people() {
    let w = world().await;
    let sub = format!("{}/subscription", w.base);
    assert_eq!(
        w.t.get(&sub, Some(&w.owner)).await.body["subscribed"],
        true,
        "creator follows"
    );
    assert_eq!(
        w.t.get(&sub, Some(&w.contrib)).await.body["subscribed"],
        false
    );
    let cid = w.t.user_id(&w.contrib).await;
    let oid = w.t.user_id(&w.outsider).await;
    let html = format!(
        r#"<p>ping <span data-type="mention" data-id="{cid}" data-label="con">@con</span> <span data-type="mention" data-id="{oid}">@out</span></p>"#
    );
    w.t.post(
        &format!("{}/comments", w.base),
        json!({ "body_html": html }),
        Some(&w.commenter),
    )
    .await;
    assert_eq!(
        w.t.get(&sub, Some(&w.contrib)).await.body["subscribed"],
        true,
        "mentioned"
    );
    assert_eq!(
        w.t.get(&sub, Some(&w.commenter)).await.body["subscribed"],
        true,
        "commenter"
    );
    let subs =
        w.t.store
            .subscribers(w.item.parse().unwrap())
            .await
            .unwrap();
    assert!(
        !subs.contains(&oid.parse().unwrap()),
        "outsider is not in the private project"
    );

    assert_eq!(
        w.t.method("DELETE", &sub, serde_json::Value::Null, Some(&w.contrib))
            .await
            .body["subscribed"],
        false
    );
    // Assigning subscribes.
    w.t.patch(&w.base, json!({ "assignee_ids": [cid] }), Some(&w.owner))
        .await;
    assert_eq!(
        w.t.get(&sub, Some(&w.contrib)).await.body["subscribed"],
        true
    );
}

#[tokio::test]
async fn reactions_toggle() {
    let w = world().await;
    let r = format!("{}/reactions", w.base);
    let a =
        w.t.post(&r, json!({ "emoji": "👍" }), Some(&w.commenter))
            .await;
    assert_eq!(a.body[0]["emoji"], "👍");
    w.t.post(&r, json!({ "emoji": "👍" }), Some(&w.owner)).await;
    assert_eq!(
        w.t.get(&r, Some(&w.owner)).await.body[0]["user_ids"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let off = w.t.post(&r, json!({ "emoji": "👍" }), Some(&w.owner)).await;
    assert_eq!(off.body[0]["user_ids"].as_array().unwrap().len(), 1);
    assert_eq!(
        w.t.post(&r, json!({ "emoji": "lol" }), Some(&w.owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn attachments_and_file_access() {
    let w = world().await;
    let a = format!("{}/attachments", w.base);
    assert_eq!(
        w.t.upload(&a, "x.txt", b"hello", &w.commenter).await.status,
        StatusCode::FORBIDDEN
    );
    let up =
        w.t.upload(&a, "../notes.txt", b"hello world", &w.contrib)
            .await;
    assert_eq!(up.status, StatusCode::CREATED, "{}", up.body);
    assert_eq!(up.body["file_name"], "notes.txt");
    assert_eq!(up.body["size"], 11);
    let fid = up.body["id"].as_str().unwrap().to_string();
    let html =
        w.t.upload(&a, "evil.html", b"<script>alert(1)</script>", &w.owner)
            .await
            .body;

    let (s, h, b) = w.t.raw(&format!("/api/v1/files/{fid}"), &w.commenter).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b, b"hello world");
    assert!(
        h["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("inline")
    );
    assert_eq!(h["x-content-type-options"], "nosniff");
    let (_, h, _) =
        w.t.raw(
            &format!("/api/v1/files/{}", html["id"].as_str().unwrap()),
            &w.owner,
        )
        .await;
    assert!(
        h["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment"),
        "HTML never inline"
    );
    assert!(
        h["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
    let (s, _, _) = w.t.raw(&format!("/api/v1/files/{fid}"), &w.outsider).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "outsider of the private project");

    // Too big; contributor cannot delete someone else's; deleted files stop serving.
    assert_eq!(
        w.t.upload(&a, "big.bin", &vec![0u8; 70 * 1024], &w.owner)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    let hid = html["id"].as_str().unwrap();
    assert_eq!(
        w.t.delete(&format!("{a}/{hid}"), Some(&w.contrib))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.t.delete(&format!("{a}/{fid}"), Some(&w.contrib))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        w.t.raw(&format!("/api/v1/files/{fid}"), &w.owner).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        w.t.get(&a, Some(&w.owner))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Editor images: images only.
    let img =
        w.t.upload(
            &format!("{}/uploads", w.pbase),
            "shot.png",
            b"\x89PNG....",
            &w.contrib,
        )
        .await;
    assert_eq!(img.status, StatusCode::CREATED);
    assert!(
        img.body["url"]
            .as_str()
            .unwrap()
            .starts_with("/api/v1/files/")
    );
    assert_eq!(
        w.t.upload(
            &format!("{}/uploads", w.pbase),
            "x.svg",
            b"<svg/>",
            &w.contrib
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn avatar_logo_and_links() {
    let w = world().await;
    let me =
        w.t.upload("/api/v1/me/avatar", "me.png", b"\x89PNG", &w.contrib)
            .await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    let url = me.body["avatar"].as_str().unwrap().to_string();
    assert_eq!(
        w.t.raw(&url, &w.outsider).await.0,
        StatusCode::OK,
        "avatars are visible to everyone logged in"
    );
    assert_eq!(
        w.t.upload(
            "/api/v1/workspaces/acme/logo",
            "l.png",
            b"\x89PNG",
            &w.contrib
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let logo =
        w.t.upload(
            "/api/v1/workspaces/acme/logo",
            "l.png",
            b"\x89PNG",
            &w.owner,
        )
        .await;
    assert_eq!(
        w.t.get("/api/v1/workspaces/acme", Some(&w.owner))
            .await
            .body["logo"],
        logo.body["logo"]
    );

    let l = format!("{}/links", w.base);
    assert_eq!(
        w.t.post(
            &l,
            json!({ "url": "javascript:alert(1)" }),
            Some(&w.contrib)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let r =
        w.t.post(
            &l,
            json!({ "url": "https://github.com/acme/web/pull/1", "title": "PR" }),
            Some(&w.contrib),
        )
        .await;
    assert_eq!(r.body[0]["title"], "PR");
    assert_eq!(
        w.t.post(&l, json!({ "url": "https://x.test" }), Some(&w.commenter))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let lid = r.body[0]["id"].as_str().unwrap();
    assert_eq!(
        w.t.delete(&format!("{l}/{lid}"), Some(&w.contrib))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
}
