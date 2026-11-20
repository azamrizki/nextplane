mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

struct W {
    t: common::TestApp,
    owner: String,
    contrib: String,
    commenter: String,
    pid: String,
    base: String,
}

async fn world() -> W {
    let t = common::app().await;
    let owner = t.setup_admin().await;
    t.workspace(&owner, "acme").await;
    let contrib = t.join(&owner, "acme", "con@example.com", "member").await;
    let commenter = t.join(&owner, "acme", "com@example.com", "member").await;
    let p = t
        .post(
            "/api/v1/workspaces/acme/projects",
            json!({ "name": "Website", "estimate_type": "points" }),
            Some(&owner),
        )
        .await
        .body;
    let pid = p["id"].as_str().unwrap().to_string();
    let base = format!("/api/v1/workspaces/acme/projects/{pid}");
    t.patch(&base, json!({ "estimate_type": "points" }), Some(&owner))
        .await;
    t.post(&format!("{base}/join"), json!({}), Some(&contrib))
        .await;
    let cid = t.user_id(&commenter).await;
    t.post(
        &format!("{base}/members"),
        json!({ "user_id": cid, "role": "commenter" }),
        Some(&owner),
    )
    .await;
    W {
        t,
        owner,
        contrib,
        commenter,
        pid,
        base,
    }
}

impl W {
    async fn item(&self, who: &str, body: Value) -> Value {
        let r = self
            .t
            .post(&format!("{}/work-items", self.base), body, Some(who))
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        r.body
    }
    async fn states(&self) -> Value {
        self.t
            .get(&format!("{}/states", self.base), Some(&self.owner))
            .await
            .body
    }
    async fn state_id(&self, name: &str) -> String {
        self.states()
            .await
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
}

#[tokio::test]
async fn create_with_sequence_defaults_and_sanitized_description() {
    let w = world().await;
    let a = w
        .item(&w.contrib, json!({ "title": "  First   item " }))
        .await;
    let b = w
        .item(
            &w.owner,
            json!({
                "title": "Second", "priority": "high", "due_date": "2026-12-01", "estimate": "5",
                "description_html": "<p>Hi<script>x</script><a href=\"javascript:1\">bad</a></p>",
                "description_json": { "type": "doc", "content": [] }
            }),
        )
        .await;
    assert_eq!(a["sequence"], 1);
    assert_eq!(b["sequence"], 2);
    assert_eq!(a["title"], "First item");
    assert_eq!(a["state_id"], w.state_id("Todo").await, "default state");
    assert_eq!(a["priority"], "none");
    let html = b["description_html"].as_str().unwrap();
    assert!(
        !html.contains("script") && !html.contains("javascript"),
        "{html}"
    );
    assert_eq!(b["description_json"]["type"], "doc");

    // Bad input.
    let base = format!("{}/work-items", w.base);
    for body in [
        json!({ "title": "   " }),
        json!({ "title": "x", "priority": "p0" }),
        json!({ "title": "x", "due_date": "2026-02-30" }),
        json!({ "title": "x", "start_date": "2026-03-02", "due_date": "2026-03-01" }),
        json!({ "title": "x", "estimate": "4" }),
    ] {
        let r = w.t.post(&base, body.clone(), Some(&w.owner)).await;
        assert!(
            r.status == StatusCode::BAD_REQUEST || r.status == StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            r.status
        );
    }
    // Assignees must be project members.
    let outsider = w.t.user("out@example.com").await;
    let oid = w.t.user_id(&outsider).await;
    assert_eq!(
        w.t.post(
            &base,
            json!({ "title": "x", "assignee_ids": [oid] }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn role_rules_on_work_items() {
    let w = world().await;
    let base = format!("{}/work-items", w.base);
    let mine = w.item(&w.contrib, json!({ "title": "Mine" })).await;
    let theirs = w.item(&w.owner, json!({ "title": "Theirs" })).await;
    let (mid, tid) = (mine["id"].as_str().unwrap(), theirs["id"].as_str().unwrap());

    // Commenter: reads, cannot create or edit.
    assert_eq!(
        w.t.get(&base, Some(&w.commenter))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        w.t.post(&base, json!({ "title": "x" }), Some(&w.commenter))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.t.patch(
            &format!("{base}/{tid}"),
            json!({ "title": "x" }),
            Some(&w.commenter)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    // Contributor: edits anyone's, deletes only own.
    assert_eq!(
        w.t.patch(
            &format!("{base}/{tid}"),
            json!({ "priority": "low" }),
            Some(&w.contrib)
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        w.t.delete(&format!("{base}/{tid}"), Some(&w.contrib))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.t.delete(&format!("{base}/{mid}"), Some(&w.contrib))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        w.t.get(&format!("{base}/{mid}"), Some(&w.owner))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    // Admin deletes anything.
    assert_eq!(
        w.t.delete(&format!("{base}/{tid}"), Some(&w.owner))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    // A workspace member who has not joined sees nothing.
    let other = w.t.join(&w.owner, "acme", "x@example.com", "member").await;
    assert_eq!(
        w.t.get(&base, Some(&other)).await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn updates_write_activity_and_completed_at() {
    let w = world().await;
    let base = format!("{}/work-items", w.base);
    let it = w.item(&w.owner, json!({ "title": "Task" })).await;
    let id = it["id"].as_str().unwrap();
    let cid = w.t.user_id(&w.contrib).await;
    let label =
        w.t.post(
            &format!("{}/labels", w.base),
            json!({ "name": "Bug", "color": "#e5484d" }),
            Some(&w.owner),
        )
        .await
        .body;
    let done = w.state_id("Done").await;

    let up = w.t.patch(&format!("{base}/{id}"), json!({
        "title": "Task 2", "state_id": done, "priority": "urgent", "assignee_ids": [cid], "label_ids": [label["id"]],
        "start_date": "2026-11-01", "due_date": "2026-11-30"
    }), Some(&w.owner)).await;
    assert_eq!(up.status, StatusCode::OK, "{}", up.body);
    assert!(up.body["completed_at"].is_string());
    assert_eq!(up.body["assignee_ids"], json!([cid]));
    let back =
        w.t.patch(
            &format!("{base}/{id}"),
            json!({ "state_id": w.state_id("Todo").await, "assignee_ids": [], "due_date": null }),
            Some(&w.owner),
        )
        .await;
    assert!(back.body["completed_at"].is_null());
    assert!(back.body["due_date"].is_null());
    assert_eq!(back.body["assignee_ids"], json!([]));
    let bad =
        w.t.patch(
            &format!("{base}/{id}"),
            json!({ "due_date": "2026-10-01" }),
            Some(&w.owner),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "due before start");

    let act =
        w.t.get(&format!("{base}/{id}/activity"), Some(&w.commenter))
            .await
            .body;
    let kinds: Vec<String> = act
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            format!(
                "{}:{}",
                a["verb"].as_str().unwrap(),
                a["field"].as_str().unwrap_or("")
            )
        })
        .collect();
    for k in [
        "created:",
        "updated:title",
        "updated:state",
        "updated:priority",
        "added:assignee",
        "added:label",
        "updated:due_date",
        "removed:assignee",
    ] {
        assert!(kinds.contains(&k.to_string()), "missing {k} in {kinds:?}");
    }
}

#[tokio::test]
async fn filters_and_search() {
    let w = world().await;
    let base = format!("{}/work-items", w.base);
    let cid = w.t.user_id(&w.contrib).await;
    w.item(&w.owner, json!({ "title": "Login bug", "priority": "urgent", "assignee_ids": [cid], "due_date": "2026-11-10" })).await;
    w.item(
        &w.owner,
        json!({ "title": "Signup page", "priority": "low" }),
    )
    .await;
    let third = w
        .item(
            &w.owner,
            json!({ "title": "Logout", "state_id": w.state_id("Done").await }),
        )
        .await;
    let n = |v: Value| v.as_array().unwrap().len();
    assert_eq!(
        n(w.t
            .get(&format!("{base}?priority=urgent,low"), Some(&w.owner))
            .await
            .body),
        2
    );
    assert_eq!(
        n(w.t
            .get(&format!("{base}?assignee={cid}"), Some(&w.owner))
            .await
            .body),
        1
    );
    assert_eq!(
        n(w.t
            .get(&format!("{base}?unassigned=true"), Some(&w.owner))
            .await
            .body),
        2
    );
    assert_eq!(
        n(w.t
            .get(&format!("{base}?state_group=completed"), Some(&w.owner))
            .await
            .body),
        1
    );
    assert_eq!(
        n(w.t.get(&format!("{base}?q=log"), Some(&w.owner)).await.body),
        2
    );
    assert_eq!(
        n(w.t
            .get(&format!("{base}?q=WEB-3"), Some(&w.owner))
            .await
            .body),
        1
    );
    assert_eq!(
        n(w.t
            .get(&format!("{base}?due_before=2026-11-30"), Some(&w.owner))
            .await
            .body),
        1
    );
    assert_eq!(
        w.t.get(&format!("{base}?priority=p9"), Some(&w.owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    // Archive hides from the normal list.
    let tid = third["id"].as_str().unwrap();
    assert_eq!(
        w.t.post(
            &format!("{base}/{tid}/archive"),
            json!({}),
            Some(&w.contrib)
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(n(w.t.get(&base, Some(&w.owner)).await.body), 2);
    assert_eq!(
        n(w.t
            .get(&format!("{base}?archived=true"), Some(&w.owner))
            .await
            .body),
        1
    );
    assert_eq!(
        w.t.post(
            &format!("{base}/{tid}/archive"),
            json!({}),
            Some(&w.commenter)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );

    // Key lookup.
    let r =
        w.t.get(
            "/api/v1/workspaces/acme/work-items/by-key/web-1",
            Some(&w.commenter),
        )
        .await;
    assert_eq!(r.body["project_id"], w.pid);
}

#[tokio::test]
async fn parents_relations_and_bulk() {
    let w = world().await;
    let base = format!("{}/work-items", w.base);
    let a = w.item(&w.owner, json!({ "title": "Epic" })).await;
    let aid = a["id"].as_str().unwrap();
    let b = w
        .item(&w.owner, json!({ "title": "Child", "parent_id": aid }))
        .await;
    let bid = b["id"].as_str().unwrap();
    let c = w
        .item(&w.owner, json!({ "title": "Grandchild", "parent_id": bid }))
        .await;
    let cid = c["id"].as_str().unwrap();

    assert_eq!(
        w.t.get(&format!("{base}/{aid}"), Some(&w.owner)).await.body["sub_item_count"],
        1
    );
    assert_eq!(
        w.t.get(&format!("{base}?parent={aid}"), Some(&w.owner))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // Loops are refused.
    assert_eq!(
        w.t.patch(
            &format!("{base}/{aid}"),
            json!({ "parent_id": cid }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        w.t.patch(
            &format!("{base}/{aid}"),
            json!({ "parent_id": aid }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    // Relations, seen from both sides.
    let r =
        w.t.post(
            &format!("{base}/{aid}/relations"),
            json!({ "kind": "blocked_by", "work_item_id": cid }),
            Some(&w.contrib),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body[0]["kind"], "blocked_by");
    assert_eq!(r.body[0]["item"]["identifier"], "WEB");
    let from_c =
        w.t.get(&format!("{base}/{cid}/relations"), Some(&w.owner))
            .await
            .body;
    assert_eq!(from_c[0]["kind"], "blocks");
    let dup =
        w.t.post(
            &format!("{base}/{cid}/relations"),
            json!({ "kind": "blocks", "work_item_id": aid }),
            Some(&w.owner),
        )
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);
    let rid = from_c[0]["id"].as_str().unwrap();
    assert_eq!(
        w.t.delete(&format!("{base}/{cid}/relations/{rid}"), Some(&w.owner))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert!(
        w.t.get(&format!("{base}/{aid}/relations"), Some(&w.owner))
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Deleting a parent frees its children.
    w.t.delete(&format!("{base}/{bid}"), Some(&w.owner)).await;
    assert!(w.t.get(&format!("{base}/{cid}"), Some(&w.owner)).await.body["parent_id"].is_null());

    // Bulk edit.
    let r =
        w.t.patch(
            &base,
            json!({ "ids": [aid, cid], "priority": "high" }),
            Some(&w.contrib),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.body
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["priority"] == "high")
    );
    assert_eq!(
        w.t.patch(
            &base,
            json!({ "ids": [aid], "title": "x" }),
            Some(&w.contrib)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        w.t.patch(
            &base,
            json!({ "ids": [aid], "priority": "low" }),
            Some(&w.commenter)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn archived_project_is_read_only_and_states_in_use_stay() {
    let w = world().await;
    let base = format!("{}/work-items", w.base);
    let it = w
        .item(
            &w.owner,
            json!({ "title": "Task", "state_id": w.state_id("In Progress").await }),
        )
        .await;
    let ip = w.state_id("In Progress").await;
    assert_eq!(
        w.t.delete(&format!("{}/states/{ip}", w.base), Some(&w.owner))
            .await
            .status,
        StatusCode::CONFLICT
    );

    w.t.post(&format!("{}/archive", w.base), json!({}), Some(&w.owner))
        .await;
    let id = it["id"].as_str().unwrap();
    assert_eq!(
        w.t.get(&format!("{base}/{id}"), Some(&w.contrib))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        w.t.patch(
            &format!("{base}/{id}"),
            json!({ "title": "x" }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        w.t.post(&base, json!({ "title": "x" }), Some(&w.owner))
            .await
            .status,
        StatusCode::CONFLICT
    );
}
