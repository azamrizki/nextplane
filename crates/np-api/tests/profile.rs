mod common;

use axum::http::{StatusCode, header};
use serde_json::{Value, json};

const H: &str = "/api/v1/workspaces/acme";

#[tokio::test]
async fn profiles_count_what_the_viewer_may_read() {
    let t = common::app().await;
    let owner = t.setup_admin().await;
    t.workspace(&owner, "acme").await;
    let mk = |name: &'static str| {
        let t = &t;
        let owner = owner.clone();
        async move {
            let p = t
                .post(&format!("{H}/projects"), json!({ "name": name }), Some(&owner))
                .await
                .body;
            format!("{H}/projects/{}", p["id"].as_str().unwrap())
        }
    };
    let (a, b) = (mk("Alpha").await, mk("Beta").await);
    let bob = t.join(&owner, "acme", "bob@example.com", "member").await;
    let bob_id = t.user_id(&bob).await;
    let owner_id = t.user_id(&owner).await;
    t.post(&format!("{a}/join"), json!({}), Some(&bob)).await;
    let states = t.get(&format!("{a}/states"), Some(&owner)).await.body;
    let done = states.as_array().unwrap().iter().find(|s| s["group"] == "completed").unwrap()["id"].clone();
    let post = |path: String, body: Value, who: String| {
        let t = &t;
        async move {
            let r = t.post(&path, body, Some(&who)).await;
            assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
            r.body
        }
    };
    post(format!("{a}/work-items"), json!({ "title": "Login", "priority": "urgent", "assignee_ids": [bob_id] }), owner.clone()).await;
    post(format!("{a}/work-items"), json!({ "title": "Docs", "state_id": done, "assignee_ids": [bob_id] }), bob.clone()).await;
    post(format!("{b}/work-items"), json!({ "title": "Secret plan", "assignee_ids": [owner_id] }), owner.clone()).await;

    let p = t
        .get(&format!("{H}/members/{bob_id}/profile"), Some(&owner))
        .await;
    assert_eq!(p.status, StatusCode::OK, "{}", p.body);
    let p = p.body;
    assert_eq!(
        (p["created"].as_i64(), p["assigned"].as_i64(), p["pending"].as_i64(), p["completed"].as_i64(), p["subscribed"].as_i64()),
        (Some(1), Some(2), Some(1), Some(1), Some(2))
    );
    assert_eq!(p["user"]["email"], "bob@example.com");
    let urgent = p["by_priority"].as_array().unwrap().iter().find(|c| c["key"] == "urgent").unwrap();
    assert_eq!(urgent["count"], 1);
    assert_eq!(p["projects"].as_array().unwrap().len(), 2);

    // Bob, looking at the owner, sees only what is in the projects he belongs to.
    let o = t
        .get(&format!("{H}/members/{owner_id}/profile"), Some(&bob))
        .await
        .body;
    assert_eq!((o["created"].as_i64(), o["assigned"].as_i64()), (Some(1), Some(0)));
    assert_eq!(o["projects"].as_array().unwrap().len(), 1);

    // Guests see the summary without email, per-project rows or activity.
    let gus = t.join(&owner, "acme", "gus@example.com", "guest").await;
    let g = t.get(&format!("{H}/members/{bob_id}/profile"), Some(&gus)).await.body;
    assert!(g["user"]["email"].is_null());
    assert!(g["projects"].is_null());
    assert_eq!(
        t.get(&format!("{H}/members/{bob_id}/activity"), Some(&gus)).await.status,
        StatusCode::FORBIDDEN
    );
    let stranger = "01a1feab-397c-7512-88f7-a4520ff9ce09";
    assert_eq!(
        t.get(&format!("{H}/members/{stranger}/profile"), Some(&owner)).await.status,
        StatusCode::NOT_FOUND
    );

    // Activity: newest first, with the item's key; a day's download is a CSV.
    let act = t
        .get(&format!("{H}/members/{bob_id}/activity"), Some(&owner))
        .await
        .body;
    let rows = act.as_array().unwrap();
    assert!(!rows.is_empty());
    assert_eq!(rows[0]["identifier"], "ALP");
    assert_eq!(rows[0]["title"], "Docs");
    let today = time::OffsetDateTime::now_utc().date();
    let day = format!("{:04}-{:02}-{:02}", today.year(), today.month() as u8, today.day());
    let (status, headers, bytes) = t
        .raw(&format!("{H}/members/{bob_id}/activity?date={day}&format=csv"), &owner)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/csv"));
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("Actor,Work item,Project"), "{text}");
    assert!(text.contains("ALP-2"), "{text}");
    assert_eq!(
        t.get(&format!("{H}/members/{bob_id}/activity?format=csv"), Some(&owner)).await.status,
        StatusCode::BAD_REQUEST
    );
}
