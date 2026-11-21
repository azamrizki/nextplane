mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

fn day(offset: i64) -> String {
    let d = time::OffsetDateTime::now_utc().date() + time::Duration::days(offset);
    format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
}

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
    t.patch(&base, json!({ "estimate_type": "points" }), Some(&owner))
        .await;
    let contrib = t.join(&owner, "acme", "con@example.com", "member").await;
    t.post(&format!("{base}/join"), json!({}), Some(&contrib))
        .await;
    (t, owner, contrib, base)
}

#[tokio::test]
async fn cycle_crud_status_overlap_and_permissions() {
    let (t, owner, contrib, base) = setup().await;
    let c = format!("{base}/cycles");
    let cur = t
        .post(
            &c,
            json!({ "name": "Sprint 1", "start_date": day(-3), "end_date": day(10) }),
            Some(&contrib),
        )
        .await;
    assert_eq!(cur.status, StatusCode::CREATED, "{}", cur.body);
    assert_eq!(cur.body["status"], "current");
    let draft = t
        .post(&c, json!({ "name": "Ideas" }), Some(&owner))
        .await
        .body;
    assert_eq!(draft["status"], "draft");
    let up = t
        .post(
            &c,
            json!({ "name": "Sprint 2", "start_date": day(11), "end_date": day(24) }),
            Some(&owner),
        )
        .await
        .body;
    assert_eq!(up["status"], "upcoming");

    let clash = t
        .post(
            &c,
            json!({ "name": "Clash", "start_date": day(10), "end_date": day(12) }),
            Some(&owner),
        )
        .await;
    assert_eq!(clash.status, StatusCode::CONFLICT);
    assert_eq!(
        t.post(
            &c,
            json!({ "name": "Half", "start_date": day(30) }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.post(
            &c,
            json!({ "name": "Back", "start_date": day(40), "end_date": day(30) }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let moved = t
        .patch(
            &format!("{c}/{}", up["id"].as_str().unwrap()),
            json!({ "start_date": day(5) }),
            Some(&owner),
        )
        .await;
    assert_eq!(
        moved.status,
        StatusCode::CONFLICT,
        "editing into an overlap"
    );

    // Delete: contributor only their own; admin any.
    let did = draft["id"].as_str().unwrap();
    assert_eq!(
        t.delete(&format!("{c}/{did}"), Some(&contrib)).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.delete(&format!("{c}/{did}"), Some(&owner)).await.status,
        StatusCode::NO_CONTENT
    );
    let cid = cur.body["id"].as_str().unwrap();
    assert_eq!(
        t.delete(&format!("{c}/{cid}"), Some(&contrib)).await.status,
        StatusCode::NO_CONTENT
    );

    // Feature off: cycles are refused.
    t.patch(&base, json!({ "feature_cycles": false }), Some(&owner))
        .await;
    assert_eq!(t.get(&c, Some(&owner)).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn items_in_cycles_progress_transfer_and_completed_cycles() {
    let (t, owner, _contrib, base) = setup().await;
    let c = format!("{base}/cycles");
    let now = t
        .post(
            &c,
            json!({ "name": "Now", "start_date": day(-2), "end_date": day(2) }),
            Some(&owner),
        )
        .await
        .body;
    let next = t
        .post(
            &c,
            json!({ "name": "Next", "start_date": day(3), "end_date": day(9) }),
            Some(&owner),
        )
        .await
        .body;
    let old = t
        .post(
            &c,
            json!({ "name": "Old", "start_date": day(-20), "end_date": day(-10) }),
            Some(&owner),
        )
        .await
        .body;
    let (now_id, next_id, old_id) = (
        now["id"].as_str().unwrap(),
        next["id"].as_str().unwrap(),
        old["id"].as_str().unwrap(),
    );
    assert_eq!(old["status"], "completed");

    let states = t.get(&format!("{base}/states"), Some(&owner)).await.body;
    let done = states
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Done")
        .unwrap()["id"]
        .clone();
    let wi = format!("{base}/work-items");
    let mut ids = Vec::new();
    for (i, est) in ["1", "3", "5"].iter().enumerate() {
        let r = t
            .post(
                &wi,
                json!({ "title": format!("Task {i}"), "cycle_id": now_id, "estimate": est }),
                Some(&owner),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        ids.push(r.body["id"].as_str().unwrap().to_string());
    }
    t.patch(
        &format!("{wi}/{}", ids[0]),
        json!({ "state_id": done }),
        Some(&owner),
    )
    .await;
    // Completed cycles take no new items.
    assert_eq!(
        t.post(
            &wi,
            json!({ "title": "x", "cycle_id": old_id }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.patch(
            &format!("{wi}/{}", ids[1]),
            json!({ "cycle_id": old_id }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    let pr = t
        .get(&format!("{c}/{now_id}/progress"), Some(&owner))
        .await
        .body;
    assert_eq!(pr["total_items"], 3);
    assert_eq!(pr["completed_items"], 1);
    assert_eq!(pr["total_points"], 9.0);
    assert_eq!(pr["completed_points"], 1.0);
    let burn = pr["burndown"].as_array().unwrap();
    assert_eq!(burn.len(), 5);
    assert_eq!(burn[0]["ideal"], 3.0);
    assert_eq!(burn[4]["ideal"], 0.0);
    assert_eq!(burn[2]["remaining"], 2, "today: one of three done");
    assert!(
        burn[4]["remaining"].is_null(),
        "future days have no actual value"
    );

    let n = |v: Value| v.as_array().unwrap().len();
    assert_eq!(
        n(t.get(&format!("{wi}?cycle={now_id}"), Some(&owner))
            .await
            .body),
        3
    );
    t.post(&wi, json!({ "title": "Loose" }), Some(&owner)).await;
    assert_eq!(
        n(t.get(&format!("{wi}?cycle=none"), Some(&owner)).await.body),
        1
    );

    let r = t
        .post(
            &format!("{c}/{now_id}/transfer"),
            json!({ "to_cycle_id": next_id }),
            Some(&owner),
        )
        .await;
    assert_eq!(r.body["moved"], 2, "only unfinished items move");
    assert_eq!(
        n(t.get(&format!("{wi}?cycle={next_id}"), Some(&owner))
            .await
            .body),
        2
    );
    let act = t
        .get(&format!("{wi}/{}/activity", ids[1]), Some(&owner))
        .await
        .body;
    assert!(
        act.as_array()
            .unwrap()
            .iter()
            .any(|a| a["field"] == "cycle")
    );

    // Deleting a cycle keeps its items.
    t.delete(&format!("{c}/{next_id}"), Some(&owner)).await;
    assert!(t.get(&format!("{wi}/{}", ids[1]), Some(&owner)).await.body["cycle_id"].is_null());
    let list = t.get(&c, Some(&owner)).await.body;
    assert_eq!(n(list), 2);
}

#[tokio::test]
async fn only_completed_cycles_are_archived_and_archived_ones_are_read_only() {
    let (t, owner, contrib, base) = setup().await;
    let c = format!("{base}/cycles");
    let cur = t
        .post(
            &c,
            json!({ "name": "Now", "start_date": day(-3), "end_date": day(4) }),
            Some(&owner),
        )
        .await
        .body;
    let old = t
        .post(
            &c,
            json!({ "name": "Done", "start_date": day(-20), "end_date": day(-6) }),
            Some(&owner),
        )
        .await
        .body;
    let (cur, old) = (cur["id"].as_str().unwrap(), old["id"].as_str().unwrap());
    assert_eq!(
        t.post(&format!("{c}/{cur}/archive"), json!({}), Some(&owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.post(&format!("{c}/{old}/archive"), json!({}), Some(&contrib))
            .await
            .status,
        StatusCode::OK
    );
    let listed = t.get(&format!("{c}?archived=true"), Some(&contrib)).await;
    assert_eq!(listed.body.as_array().unwrap().len(), 1);
    for r in [
        t.patch(&format!("{c}/{old}"), json!({ "name": "x" }), Some(&owner))
            .await,
        t.delete(&format!("{c}/{old}"), Some(&owner)).await,
    ] {
        assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.body);
    }
    // Commenters do not see the archive.
    let com = t.join(&owner, "acme", "com@example.com", "member").await;
    t.post(&format!("{base}/join"), json!({}), Some(&com)).await;
    let com_id = t.user_id(&com).await;
    t.patch(
        &format!("{base}/members/{com_id}"),
        json!({ "role": "commenter" }),
        Some(&owner),
    )
    .await;
    assert_eq!(
        t.get(&format!("{c}?archived=true"), Some(&com)).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.post(&format!("{c}/{old}/unarchive"), json!({}), Some(&contrib))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        t.patch(&format!("{c}/{old}"), json!({ "name": "Done 1" }), Some(&owner))
            .await
            .status,
        StatusCode::OK
    );
}
