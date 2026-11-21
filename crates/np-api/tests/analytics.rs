mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

fn bucket<'a>(b: &'a Value, key: &str) -> &'a Value {
    b["x"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["key"] == key)
        .unwrap_or(&Value::Null)
}

#[tokio::test]
async fn numbers_follow_what_you_may_read() {
    let t = common::app().await;
    let owner = t.setup_admin().await;
    t.workspace(&owner, "acme").await;
    let a = t
        .post(
            "/api/v1/workspaces/acme/projects",
            json!({ "name": "Website" }),
            Some(&owner),
        )
        .await
        .body;
    let b = t
        .post(
            "/api/v1/workspaces/acme/projects",
            json!({ "name": "Secret", "is_private": true }),
            Some(&owner),
        )
        .await
        .body;
    let (aid, bid) = (
        a["id"].as_str().unwrap().to_string(),
        b["id"].as_str().unwrap().to_string(),
    );
    let base = format!("/api/v1/workspaces/acme/projects/{aid}");
    t.patch(&base, json!({ "estimate_type": "points" }), Some(&owner))
        .await;
    let con = t.join(&owner, "acme", "con@example.com", "member").await;
    let con_id = t.user_id(&con).await;
    t.post(
        &format!("{base}/members"),
        json!({ "user_id": con_id, "role": "contributor" }),
        Some(&owner),
    )
    .await;
    let guest = t.join(&owner, "acme", "guest@example.com", "guest").await;

    let states = t.get(&format!("{base}/states"), Some(&owner)).await.body;
    let done = states
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["group"] == "completed")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let labels = t
        .post(
            &format!("{base}/labels"),
            json!({ "name": "Bug", "color": "#e5484d" }),
            Some(&owner),
        )
        .await
        .body;
    let bug = labels["id"].as_str().unwrap().to_string();
    let items = base.clone() + "/work-items";
    t.post(&items, json!({ "title": "A", "priority": "urgent", "assignee_ids": [con_id], "label_ids": [bug], "estimate": "3" }), Some(&owner)).await;
    t.post(
        &items,
        json!({ "title": "B", "priority": "urgent", "estimate": "5" }),
        Some(&owner),
    )
    .await;
    t.post(&items, json!({ "title": "C", "priority": "low", "state_id": done, "label_ids": [bug], "estimate": "2", "due_date": "2000-01-01" }), Some(&owner)).await;
    t.post(
        &items,
        json!({ "title": "Late", "priority": "high", "due_date": "2000-01-01" }),
        Some(&owner),
    )
    .await;
    // Not counted: a draft, an archived item, another project's item the contributor cannot read.
    t.post(
        &items,
        json!({ "title": "Draft", "is_draft": true }),
        Some(&owner),
    )
    .await;
    let arch = t
        .post(&items, json!({ "title": "Old" }), Some(&owner))
        .await
        .body;
    t.complete(&format!("{items}/{}", arch["id"].as_str().unwrap()), &owner)
        .await;
    t.post(
        &format!("{items}/{}/archive", arch["id"].as_str().unwrap()),
        json!({}),
        Some(&owner),
    )
    .await;
    t.post(
        &format!("/api/v1/workspaces/acme/projects/{bid}/work-items"),
        json!({ "title": "Hidden" }),
        Some(&owner),
    )
    .await;

    let o = t
        .get(
            &format!("/api/v1/workspaces/acme/analytics/overview?project={aid}"),
            Some(&con),
        )
        .await;
    assert_eq!(o.status, StatusCode::OK, "{}", o.body);
    let o = o.body;
    assert_eq!(
        (
            o["total"].as_i64(),
            o["open"].as_i64(),
            o["completed_30d"].as_i64(),
            o["overdue"].as_i64()
        ),
        (Some(4), Some(3), Some(1), Some(1)),
        "{o}"
    );
    assert_eq!(
        (o["points_total"].as_f64(), o["points_open"].as_f64()),
        (Some(10.0), Some(8.0))
    );
    let weeks = o["weeks"].as_array().unwrap();
    assert_eq!(weeks.len(), 12);
    // This week: four live and one archived, which was completed before it could be archived
    // (the draft and the other project's item do not count).
    assert_eq!(
        (
            weeks[11]["created"].as_i64(),
            weeks[11]["completed"].as_i64()
        ),
        (Some(5), Some(2)),
        "{o}"
    );
    // State groups in their natural order, assignees largest first with "unassigned" last.
    let groups: Vec<&str> = o["by_state_group"]["x"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["key"].as_str().unwrap())
        .collect();
    assert_eq!(groups, ["unstarted", "completed"]);
    let prios: Vec<&str> = o["by_priority"]["x"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["key"].as_str().unwrap())
        .collect();
    assert_eq!(prios, ["urgent", "high", "low"]);
    let who: Vec<(&str, f64)> = o["by_assignee"]["x"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["label"].as_str().unwrap(), b["total"].as_f64().unwrap()))
        .collect();
    assert_eq!(who.last().unwrap(), &("", 3.0));

    // Custom: points by label, segmented by priority.
    let c = t.get(&format!("/api/v1/workspaces/acme/analytics?project={aid}&x=label&segment=priority&measure=points"), Some(&con)).await.body;
    assert_eq!(bucket(&c, "Bug")["total"], 5.0, "{c}");
    assert_eq!(bucket(&c, "")["total"], 5.0);
    let cell = c["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["x"] == "Bug" && x["segment"] == "low")
        .unwrap();
    assert_eq!(cell["value"], 2.0);
    assert_eq!(c["total"], 10.0);

    // Workspace-wide: the owner sees both projects, the contributor only the one they read.
    let all_owner = t
        .get("/api/v1/workspaces/acme/analytics?x=project", Some(&owner))
        .await
        .body;
    assert_eq!(all_owner["x"].as_array().unwrap().len(), 2);
    let all_con = t
        .get("/api/v1/workspaces/acme/analytics?x=project", Some(&con))
        .await
        .body;
    assert_eq!(all_con["x"].as_array().unwrap().len(), 1);
    assert_eq!(
        t.get(
            &format!("/api/v1/workspaces/acme/analytics?x=project&project={bid}"),
            Some(&con)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        t.get("/api/v1/workspaces/acme/analytics/overview", Some(&guest))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.get("/api/v1/workspaces/acme/analytics?x=nope", Some(&owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.get(
            "/api/v1/workspaces/acme/analytics?x=label&segment=label",
            Some(&owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
}
