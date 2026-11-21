mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};

#[tokio::test]
async fn export_and_import() {
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
    let com = t.join(&owner, "acme", "com@example.com", "member").await;
    let com_id = t.user_id(&com).await;
    t.post(
        &format!("{base}/members"),
        json!({ "user_id": com_id, "role": "commenter" }),
        Some(&owner),
    )
    .await;
    t.post(
        &format!("{base}/labels"),
        json!({ "name": "Bug", "color": "#e5484d" }),
        Some(&owner),
    )
    .await;

    // Import: preview guesses the mapping; the import reports what it could not use.
    let csv = "\u{feff}Summary;Status;Priority;Tags;Due;Points;Assigned to;Notes\n\
               Fix login;In Progress;High;Bug;2026-12-01;3;com@example.com;\"Steps:\n1. open\"\n\
               ;Todo;;;;;;\n\
               =HYPERLINK(\"x\");Done;urgent;Feature, Bug;31/12/2026;4;nobody@example.com;\n";
    let req = |cookie: &str, body: &str| {
        Request::post(format!("{base}/import/preview"))
            .header(header::CONTENT_TYPE, "text/csv")
            .header(header::HOST, "localhost:12313")
            .header(header::COOKIE, cookie)
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    assert_eq!(
        t.send(req(&com, csv)).await.status,
        StatusCode::FORBIDDEN,
        "commenters cannot import"
    );
    let pv = t.send(req(&owner, csv)).await;
    assert_eq!(pv.status, StatusCode::OK, "{}", pv.body);
    assert_eq!(pv.body["total_rows"], 3);
    let m = &pv.body["mapping"];
    assert_eq!(
        (
            m["title"].as_u64(),
            m["state"].as_u64(),
            m["labels"].as_u64(),
            m["due_date"].as_u64(),
            m["estimate"].as_u64(),
            m["assignees"].as_u64(),
            m["description"].as_u64()
        ),
        (
            Some(0),
            Some(1),
            Some(3),
            Some(4),
            Some(5),
            Some(6),
            Some(7)
        )
    );
    let mut mapping = m.clone();
    mapping["priority"] = json!(2);
    let r = t
        .post(
            &format!("{base}/import"),
            json!({ "csv": csv, "mapping": mapping }),
            Some(&owner),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["created"], 2, "{}", r.body);
    assert_eq!(
        r.body["skipped"][0]["line"], 4,
        "the row without a title: {}",
        r.body
    );
    let warnings: Vec<&str> = r.body["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["message"].as_str().unwrap())
        .collect();
    assert!(
        warnings.iter().any(|w| w.contains("Feature")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("31/12/2026")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("nobody@example.com")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("\"4\"")),
        "4 is not a point value: {warnings:?}"
    );
    let items = t
        .get(&format!("{base}/work-items"), Some(&owner))
        .await
        .body;
    let fix = items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["title"] == "Fix login")
        .unwrap()
        .clone();
    assert_eq!(
        (
            fix["priority"].as_str(),
            fix["due_date"].as_str(),
            fix["estimate"].as_str()
        ),
        (Some("high"), Some("2026-12-01"), Some("3"))
    );
    assert_eq!(fix["assignee_ids"][0], com_id.as_str());

    // Export: CSV with a BOM, formulas defused, and JSON with the HTML description.
    let (status, headers, bytes) = t.raw(&format!("{base}/export?format=csv"), &com).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"WEB-work-items.csv\""
    );
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.starts_with("\u{feff}Key,Title,State"), "{text}");
    assert!(
        text.contains("WEB-1,Fix login,In Progress,started,high,com@example.com,Bug"),
        "{text}"
    );
    assert!(text.contains("'=HYPERLINK"), "formulas are defused: {text}");
    assert!(text.contains("\"Steps:\n1. open\""), "{text}");
    let (_, _, json_bytes) = t.raw(&format!("{base}/export?format=json"), &com).await;
    let rows: Value = serde_json::from_slice(&json_bytes).unwrap();
    assert_eq!(rows[0]["description_html"], "<p>Steps:<br>1. open</p>");
    assert_eq!(rows[1]["state_group"], "completed");
}

#[tokio::test]
async fn workspace_exports_span_your_projects_in_three_formats() {
    let t = common::app().await;
    let owner = t.setup_admin().await;
    t.workspace(&owner, "acme").await;
    let mut bases = Vec::new();
    for name in ["Alpha", "Beta"] {
        let p = t
            .post("/api/v1/workspaces/acme/projects", json!({ "name": name }), Some(&owner))
            .await
            .body;
        let base = format!("/api/v1/workspaces/acme/projects/{}", p["id"].as_str().unwrap());
        for i in 0..2 {
            t.post(&format!("{base}/work-items"), json!({ "title": format!("{name} {i}") }), Some(&owner))
                .await;
        }
        bases.push((p["id"].as_str().unwrap().to_string(), base));
    }
    let bob = t.join(&owner, "acme", "bob@example.com", "member").await;
    t.post(&format!("{}/join", bases[0].1), json!({}), Some(&bob)).await;
    let ex = "/api/v1/workspaces/acme/exports";

    let e = t.post(ex, json!({ "format": "csv" }), Some(&owner)).await;
    assert_eq!(e.status, StatusCode::CREATED, "{}", e.body);
    assert_eq!(e.body["items"], 4);
    let (status, headers, body) = t
        .raw(&format!("{ex}/{}/download", e.body["id"].as_str().unwrap()), &owner)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers["content-disposition"].to_str().unwrap().contains("acme-work-items-"));
    let text = String::from_utf8(body).unwrap();
    assert!(text.contains("Project,Project key,Key,Title"), "{text}");
    assert!(text.contains("Alpha,ALP,ALP-1,Alpha 0") && text.contains("Beta,BET,BET-2,Beta 1"), "{text}");

    let x = t.post(ex, json!({ "format": "xlsx" }), Some(&owner)).await.body;
    let (_, headers, body) = t.raw(&format!("{ex}/{}/download", x["id"].as_str().unwrap()), &owner).await;
    assert!(headers["content-type"].to_str().unwrap().contains("spreadsheetml"));
    assert_eq!(&body[..2], b"PK");
    let j = t.post(ex, json!({ "format": "json", "project_ids": [bases[1].0] }), Some(&owner)).await.body;
    assert_eq!(j["items"], 2);
    let (_, _, body) = t.raw(&format!("{ex}/{}/download", j["id"].as_str().unwrap()), &owner).await;
    let rows: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["project"], "Beta");

    // Only projects you belong to as a contributor or admin; your own history only.
    assert_eq!(
        t.post(ex, json!({ "format": "csv", "project_ids": [bases[1].0] }), Some(&bob)).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(t.post(ex, json!({ "format": "csv" }), Some(&bob)).await.body["items"], 2);
    assert_eq!(t.post(ex, json!({ "format": "pdf" }), Some(&bob)).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(t.get(ex, Some(&owner)).await.body.as_array().unwrap().len(), 3);
    assert_eq!(t.get(ex, Some(&bob)).await.body.as_array().unwrap().len(), 1);
    let gus = t.join(&owner, "acme", "gus@example.com", "guest").await;
    assert_eq!(t.post(ex, json!({ "format": "csv" }), Some(&gus)).await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        t.raw(&format!("{ex}/{}/download", e.body["id"].as_str().unwrap()), &bob).await.0,
        StatusCode::NOT_FOUND
    );
}
