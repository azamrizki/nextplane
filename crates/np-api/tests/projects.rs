mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

struct World {
    t: common::TestApp,
    owner: String,
    admin: String,
    member: String,
    member2: String,
    guest: String,
}

async fn world() -> World {
    let t = common::app().await;
    let owner = t.setup_admin().await;
    t.workspace(&owner, "acme").await;
    let admin = t.join(&owner, "acme", "adm@example.com", "admin").await;
    let member = t.join(&owner, "acme", "mem@example.com", "member").await;
    let member2 = t.join(&owner, "acme", "mem2@example.com", "member").await;
    let guest = t.join(&owner, "acme", "gus@example.com", "guest").await;
    World {
        t,
        owner,
        admin,
        member,
        member2,
        guest,
    }
}

const P: &str = "/api/v1/workspaces/acme/projects";

async fn create(w: &World, who: &str, body: Value) -> Value {
    let r = w.t.post(P, body, Some(who)).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    r.body
}

#[tokio::test]
async fn create_project_with_defaults_and_rules() {
    let w = world().await;
    let p = create(&w, &w.owner, json!({ "name": "Website" })).await;
    assert_eq!(p["identifier"], "WEB");
    assert_eq!(p["my_role"], "admin");
    assert_eq!(p["is_private"], false);
    assert_eq!(p["estimate_type"], "none");
    let id = p["id"].as_str().unwrap();

    let states =
        w.t.get(&format!("{P}/{id}/states"), Some(&w.owner))
            .await
            .body;
    let names: Vec<&str> = states
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["Backlog", "Todo", "In Progress", "Done", "Cancelled"]
    );
    assert_eq!(states[1]["is_default"], true);
    assert_eq!(states[2]["group"], "started");

    // Unique identifier and name; bad identifier.
    assert_eq!(
        w.t.post(
            P,
            json!({ "name": "Other", "identifier": "WEB" }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        w.t.post(P, json!({ "name": "website" }), Some(&w.owner))
            .await
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        w.t.post(
            P,
            json!({ "name": "X", "identifier": "1AB" }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    // Members create projects only when the workspace allows it; guests never.
    assert_eq!(
        w.t.post(P, json!({ "name": "M" }), Some(&w.member))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    w.t.patch(
        "/api/v1/workspaces/acme",
        json!({ "members_can_create_projects": true }),
        Some(&w.owner),
    )
    .await;
    create(&w, &w.member, json!({ "name": "Mobile App" })).await;
    assert_eq!(
        w.t.post(P, json!({ "name": "G" }), Some(&w.guest))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    create(
        &w,
        &w.admin,
        json!({ "name": "Admin made", "identifier": "ADM" }),
    )
    .await;
}

#[tokio::test]
async fn visibility_join_and_guest_ceiling() {
    let w = world().await;
    w.t.patch(
        "/api/v1/workspaces/acme",
        json!({ "members_can_create_projects": true }),
        Some(&w.owner),
    )
    .await;
    let public = create(
        &w,
        &w.member,
        json!({ "name": "Public2", "identifier": "PUB" }),
    )
    .await;
    let private = create(
        &w,
        &w.member,
        json!({ "name": "Secret", "identifier": "SEC", "is_private": true }),
    )
    .await;
    let (pub_id, priv_id) = (
        public["id"].as_str().unwrap(),
        private["id"].as_str().unwrap(),
    );

    let names = |v: &Value| {
        v.as_array()
            .unwrap()
            .iter()
            .map(|p| p["identifier"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    // member2: sees the public one (not joined), not the private one (D4).
    let list = w.t.get(P, Some(&w.member2)).await.body;
    assert!(names(&list).contains(&"PUB".to_string()));
    assert!(!names(&list).contains(&"SEC".to_string()));
    assert_eq!(
        w.t.get(&format!("{P}/{priv_id}"), Some(&w.member2))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    // Workspace admin sees everything and is an implicit project admin.
    let adm = w.t.get(&format!("{P}/{priv_id}"), Some(&w.admin)).await;
    assert_eq!(adm.body["my_role"], "admin");
    assert_eq!(adm.body["is_member"], false);
    // Guests see nothing they were not added to.
    assert!(names(&w.t.get(P, Some(&w.guest)).await.body).is_empty());

    // Not joined: no states; join as contributor.
    assert_eq!(
        w.t.get(&format!("{P}/{pub_id}/states"), Some(&w.member2))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let j =
        w.t.post(&format!("{P}/{pub_id}/join"), json!({}), Some(&w.member2))
            .await;
    assert_eq!(j.body["my_role"], "contributor");
    assert_eq!(
        w.t.post(&format!("{P}/{priv_id}/join"), json!({}), Some(&w.member2))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        w.t.post(&format!("{P}/{pub_id}/join"), json!({}), Some(&w.guest))
            .await
            .status,
        StatusCode::NOT_FOUND,
        "D3"
    );

    // Adding the guest: commenter ok, contributor refused (ceiling).
    let gid = w.t.user_id(&w.guest).await;
    let r =
        w.t.post(
            &format!("{P}/{priv_id}/members"),
            json!({ "user_id": gid, "role": "contributor" }),
            Some(&w.member),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r =
        w.t.post(
            &format!("{P}/{priv_id}/members"),
            json!({ "user_id": gid, "role": "commenter" }),
            Some(&w.member),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(
        w.t.get(&format!("{P}/{priv_id}"), Some(&w.guest))
            .await
            .body["my_role"],
        "commenter"
    );
    // A non-admin cannot add members.
    let m2 = w.t.user_id(&w.member2).await;
    let r =
        w.t.post(
            &format!("{P}/{priv_id}/members"),
            json!({ "user_id": m2, "role": "guest" }),
            Some(&w.guest),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn last_admin_rules_and_workspace_removal() {
    let w = world().await;
    w.t.patch(
        "/api/v1/workspaces/acme",
        json!({ "members_can_create_projects": true }),
        Some(&w.owner),
    )
    .await;
    let p = create(&w, &w.member, json!({ "name": "Solo" })).await;
    let id = p["id"].as_str().unwrap();
    let me = w.t.user_id(&w.member).await;

    assert_eq!(
        w.t.post(&format!("{P}/{id}/leave"), json!({}), Some(&w.member))
            .await
            .status,
        StatusCode::CONFLICT
    );
    let r =
        w.t.patch(
            &format!("{P}/{id}/members/{me}"),
            json!({ "role": "contributor" }),
            Some(&w.member),
        )
        .await;
    assert_eq!(
        r.status,
        StatusCode::CONFLICT,
        "D6: last admin cannot be demoted"
    );
    // The workspace cannot remove someone who is a project's only admin.
    let r =
        w.t.delete(
            &format!("/api/v1/workspaces/acme/members/{me}"),
            Some(&w.owner),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert!(
        r.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Solo")
    );

    // With a second admin both work.
    let m2 = w.t.user_id(&w.member2).await;
    w.t.post(
        &format!("{P}/{id}/members"),
        json!({ "user_id": m2, "role": "admin" }),
        Some(&w.member),
    )
    .await;
    assert_eq!(
        w.t.post(&format!("{P}/{id}/leave"), json!({}), Some(&w.member))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let members =
        w.t.get(&format!("{P}/{id}/members"), Some(&w.member2))
            .await
            .body;
    assert_eq!(members.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn settings_archive_delete_only_for_admins() {
    let w = world().await;
    let p = create(&w, &w.owner, json!({ "name": "Website" })).await;
    let id = p["id"].as_str().unwrap();
    w.t.post(&format!("{P}/{id}/join"), json!({}), Some(&w.member))
        .await;

    assert_eq!(
        w.t.patch(
            &format!("{P}/{id}"),
            json!({ "name": "X" }),
            Some(&w.member)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let up =
        w.t.patch(
            &format!("{P}/{id}"),
            json!({
                "name": "Web", "identifier": "WWW", "icon": "🌐", "cover_color": "#0E7C6B",
                "estimate_type": "points", "feature_intake": true, "is_private": true
            }),
            Some(&w.owner),
        )
        .await;
    assert_eq!(up.status, StatusCode::OK, "{}", up.body);
    assert_eq!(up.body["identifier"], "WWW");
    assert_eq!(
        up.body["estimate_values"],
        json!(["1", "2", "3", "5", "8", "13"])
    );
    assert_eq!(up.body["cover_color"], "#0e7c6b");
    let bad =
        w.t.patch(
            &format!("{P}/{id}"),
            json!({ "estimate_type": "points", "estimate_values": ["a", "b"] }),
            Some(&w.owner),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let gus = w.t.user_id(&w.guest).await;
    let bad =
        w.t.patch(
            &format!("{P}/{id}"),
            json!({ "default_assignee_id": gus }),
            Some(&w.owner),
        )
        .await;
    assert_eq!(
        bad.status,
        StatusCode::BAD_REQUEST,
        "default assignee must be a project member"
    );

    assert_eq!(
        w.t.post(&format!("{P}/{id}/archive"), json!({}), Some(&w.member))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.t.post(&format!("{P}/{id}/archive"), json!({}), Some(&w.owner))
            .await
            .status,
        StatusCode::OK
    );
    assert!(
        w.t.get(P, Some(&w.owner))
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        w.t.get(&format!("{P}?archived=true"), Some(&w.owner))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        w.t.post(&format!("{P}/{id}/unarchive"), json!({}), Some(&w.owner))
            .await
            .status,
        StatusCode::OK
    );

    assert_eq!(
        w.t.delete(&format!("{P}/{id}"), Some(&w.member))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.t.delete(&format!("{P}/{id}"), Some(&w.admin))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        w.t.get(&format!("{P}/{id}"), Some(&w.owner)).await.status,
        StatusCode::NOT_FOUND
    );
    // The identifier is free again.
    create(&w, &w.owner, json!({ "name": "Web", "identifier": "WWW" })).await;
}

#[tokio::test]
async fn states_and_labels() {
    let w = world().await;
    let p = create(&w, &w.owner, json!({ "name": "Website" })).await;
    let id = p["id"].as_str().unwrap();
    w.t.post(&format!("{P}/{id}/join"), json!({}), Some(&w.member))
        .await;
    let s = format!("{P}/{id}/states");

    assert_eq!(
        w.t.post(
            &s,
            json!({ "name": "Review", "color": "#123456", "group": "started" }),
            Some(&w.member)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let review =
        w.t.post(
            &s,
            json!({ "name": "Review", "color": "#123456", "group": "started" }),
            Some(&w.owner),
        )
        .await;
    assert_eq!(review.status, StatusCode::CREATED);
    assert_eq!(
        w.t.post(
            &s,
            json!({ "name": "review", "color": "#123456", "group": "started" }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    let rid = review.body["id"].as_str().unwrap();

    let all =
        w.t.patch(
            &format!("{s}/{rid}"),
            json!({ "is_default": true, "name": "In Review" }),
            Some(&w.owner),
        )
        .await
        .body;
    let defaults: Vec<&Value> = all
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["is_default"] == true)
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0]["name"], "In Review");
    assert_eq!(
        w.t.delete(&format!("{s}/{rid}"), Some(&w.owner))
            .await
            .status,
        StatusCode::CONFLICT,
        "default state"
    );
    let todo = all
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Todo")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        w.t.delete(&format!("{s}/{todo}"), Some(&w.owner))
            .await
            .status,
        StatusCode::NO_CONTENT
    );

    let l = format!("{P}/{id}/labels");
    let bug =
        w.t.post(
            &l,
            json!({ "name": "Bug", "color": "#e5484d" }),
            Some(&w.owner),
        )
        .await
        .body;
    let bug_id = bug["id"].as_str().unwrap();
    let ui =
        w.t.post(
            &l,
            json!({ "name": "UI", "color": "#3e63dd", "parent_id": bug_id }),
            Some(&w.owner),
        )
        .await;
    assert_eq!(ui.status, StatusCode::CREATED);
    let ui_id = ui.body["id"].as_str().unwrap();
    let deep =
        w.t.post(
            &l,
            json!({ "name": "Deep", "color": "#3e63dd", "parent_id": ui_id }),
            Some(&w.owner),
        )
        .await;
    assert_eq!(deep.status, StatusCode::BAD_REQUEST, "one level of nesting");
    assert_eq!(
        w.t.get(&l, Some(&w.member))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        w.t.delete(&format!("{l}/{bug_id}"), Some(&w.owner))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let left = w.t.get(&l, Some(&w.member)).await.body;
    assert_eq!(left[0]["name"], "UI");
    assert!(
        left[0]["parent_id"].is_null(),
        "children move to the top level"
    );
}
