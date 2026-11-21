mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

#[tokio::test]
async fn modules_items_progress_links_and_rules() {
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
    let contrib = t.join(&owner, "acme", "con@example.com", "member").await;
    t.post(&format!("{base}/join"), json!({}), Some(&contrib))
        .await;
    let cid = t.user_id(&contrib).await;
    let m = format!("{base}/modules");

    let a = t.post(&m, json!({ "name": "Checkout", "status": "planned", "start_date": "2026-11-01", "target_date": "2026-12-15", "member_ids": [cid] }), Some(&contrib)).await;
    assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
    assert_eq!(a.body["lead_id"], cid, "creator leads by default");
    assert_eq!(a.body["member_ids"], json!([cid]));
    let b = t
        .post(&m, json!({ "name": "Search" }), Some(&owner))
        .await
        .body;
    assert_eq!(
        t.post(&m, json!({ "name": "checkout" }), Some(&owner))
            .await
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        t.post(&m, json!({ "name": "X", "status": "done" }), Some(&owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.post(
            &m,
            json!({ "name": "Y", "start_date": "2026-12-02", "target_date": "2026-12-01" }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let (aid, bid) = (
        a.body["id"].as_str().unwrap().to_string(),
        b["id"].as_str().unwrap().to_string(),
    );

    // Items in several modules.
    let wi = format!("{base}/work-items");
    let one = t
        .post(
            &wi,
            json!({ "title": "Pay with card", "module_ids": [aid, bid] }),
            Some(&owner),
        )
        .await
        .body;
    assert_eq!(one["module_ids"].as_array().unwrap().len(), 2);
    let two = t
        .post(&wi, json!({ "title": "Coupon field" }), Some(&owner))
        .await
        .body;
    let two_id = two["id"].as_str().unwrap();
    let up = t
        .patch(
            &format!("{wi}/{two_id}"),
            json!({ "module_ids": [aid] }),
            Some(&contrib),
        )
        .await;
    assert_eq!(up.body["module_ids"], json!([aid]));
    let n = |v: Value| v.as_array().unwrap().len();
    assert_eq!(
        n(t.get(&format!("{wi}?module={aid}"), Some(&owner))
            .await
            .body),
        2
    );
    assert_eq!(
        n(t.get(&format!("{wi}?module={bid}"), Some(&owner))
            .await
            .body),
        1
    );
    t.post(&wi, json!({ "title": "Loose" }), Some(&owner)).await;
    assert_eq!(
        n(t.get(&format!("{wi}?module=none"), Some(&owner)).await.body),
        1
    );
    let act = t
        .get(&format!("{wi}/{two_id}/activity"), Some(&owner))
        .await
        .body;
    assert!(
        act.as_array()
            .unwrap()
            .iter()
            .any(|x| x["field"] == "module" && x["verb"] == "added")
    );

    let states = t.get(&format!("{base}/states"), Some(&owner)).await.body;
    let done = states
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Done")
        .unwrap()["id"]
        .clone();
    t.patch(
        &format!("{wi}/{two_id}"),
        json!({ "state_id": done }),
        Some(&owner),
    )
    .await;
    let pr = t
        .get(&format!("{m}/{aid}/progress"), Some(&owner))
        .await
        .body;
    assert_eq!(
        (pr["total_items"].as_i64(), pr["completed_items"].as_i64()),
        (Some(2), Some(1))
    );
    assert_eq!(pr["burndown"].as_array().unwrap().len(), 45);
    let list = t.get(&m, Some(&owner)).await.body;
    assert_eq!(list[0]["total_items"], 2);

    // Links.
    let l = t
        .post(
            &format!("{m}/{aid}/links"),
            json!({ "url": "https://figma.com/x", "title": "Designs" }),
            Some(&contrib),
        )
        .await
        .body;
    assert_eq!(l[0]["title"], "Designs");
    assert_eq!(
        t.post(
            &format!("{m}/{aid}/links"),
            json!({ "url": "ftp://x" }),
            Some(&contrib)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    // Only completed or cancelled modules are archived; archived ones take no new items and
    // are read-only until restored. Delete keeps the items.
    assert_eq!(
        t.post(&format!("{m}/{bid}/archive"), json!({}), Some(&owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    t.patch(
        &format!("{m}/{bid}"),
        json!({ "status": "cancelled" }),
        Some(&owner),
    )
    .await;
    assert_eq!(
        t.post(&format!("{m}/{bid}/archive"), json!({}), Some(&owner))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        t.get(&format!("{m}?archived=true"), Some(&owner))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for r in [
        t.patch(&format!("{m}/{bid}"), json!({ "name": "y" }), Some(&owner))
            .await,
        t.delete(&format!("{m}/{bid}"), Some(&owner)).await,
        t.post(
            &format!("{m}/{bid}/links"),
            json!({ "url": "https://x.io" }),
            Some(&owner),
        )
        .await,
    ] {
        assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.body);
    }
    assert_eq!(
        t.post(
            &wi,
            json!({ "title": "x", "module_ids": [bid] }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    t.post(&format!("{m}/{bid}/unarchive"), json!({}), Some(&owner))
        .await;
    assert_eq!(
        t.delete(&format!("{m}/{bid}"), Some(&contrib)).await.status,
        StatusCode::FORBIDDEN,
        "not theirs"
    );
    assert_eq!(
        t.delete(&format!("{m}/{aid}"), Some(&contrib)).await.status,
        StatusCode::NO_CONTENT
    );
    let item = t.get(&format!("{wi}/{two_id}"), Some(&owner)).await;
    assert_eq!(item.status, StatusCode::OK);
    assert_eq!(item.body["module_ids"], json!([]));

    t.patch(&base, json!({ "feature_modules": false }), Some(&owner))
        .await;
    assert_eq!(t.get(&m, Some(&owner)).await.status, StatusCode::FORBIDDEN);
}
