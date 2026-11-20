mod common;

use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn create_list_get_update_workspace() {
    let t = common::app().await;
    let ana = t.setup_admin().await;

    let r = t
        .post(
            "/api/v1/workspaces",
            json!({ "name": "Acme Corp" }),
            Some(&ana),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    assert_eq!(r.body["slug"], "acme-corp");
    assert_eq!(r.body["role"], "owner");

    let dup = t
        .post(
            "/api/v1/workspaces",
            json!({ "name": "Other", "slug": "acme-corp" }),
            Some(&ana),
        )
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);
    let bad = t
        .post(
            "/api/v1/workspaces",
            json!({ "name": "X", "slug": "api" }),
            Some(&ana),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    let check = t
        .get("/api/v1/workspaces/slug-check?slug=acme-corp", Some(&ana))
        .await;
    assert_eq!(check.body["available"], false);
    let check = t
        .get("/api/v1/workspaces/slug-check?slug=fresh-one", Some(&ana))
        .await;
    assert_eq!(check.body["available"], true);

    let list = t.get("/api/v1/workspaces", Some(&ana)).await;
    assert_eq!(list.body.as_array().unwrap().len(), 1);

    let up = t
        .patch(
            "/api/v1/workspaces/acme-corp",
            json!({ "name": "Acme", "slug": "acme", "members_can_create_projects": true }),
            Some(&ana),
        )
        .await;
    assert_eq!(up.status, StatusCode::OK, "{}", up.body);
    assert_eq!(up.body["slug"], "acme");
    assert_eq!(up.body["members_can_create_projects"], true);
    assert_eq!(
        t.get("/api/v1/workspaces/acme-corp", Some(&ana))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        t.get("/api/v1/workspaces/acme", Some(&ana)).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn non_members_get_404_and_members_cannot_edit() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    t.workspace(&ana, "acme").await;
    let bob = t.user("bob@example.com").await;
    assert_eq!(
        t.get("/api/v1/workspaces/acme", Some(&bob)).await.status,
        StatusCode::NOT_FOUND
    );

    let cara = t.join(&ana, "acme", "cara@example.com", "member").await;
    assert_eq!(
        t.get("/api/v1/workspaces/acme", Some(&cara)).await.body["role"],
        "member"
    );
    assert_eq!(
        t.patch(
            "/api/v1/workspaces/acme",
            json!({ "name": "X" }),
            Some(&cara)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.delete("/api/v1/workspaces/acme", Some(&cara))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.get("/api/v1/workspaces/acme/members", Some(&cara))
            .await
            .status,
        StatusCode::OK
    );

    let gus = t.join(&ana, "acme", "gus@example.com", "guest").await;
    assert_eq!(
        t.get("/api/v1/workspaces/acme/members", Some(&gus))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn trash_and_restore() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    t.workspace(&ana, "acme").await;
    let adm = t.join(&ana, "acme", "adm@example.com", "admin").await;
    assert_eq!(
        t.delete("/api/v1/workspaces/acme", Some(&adm)).await.status,
        StatusCode::FORBIDDEN,
        "only owners delete"
    );

    assert_eq!(
        t.delete("/api/v1/workspaces/acme", Some(&ana)).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        t.get("/api/v1/workspaces/acme", Some(&ana)).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        t.get("/api/v1/workspaces/trash", Some(&ana)).await.body[0]["slug"],
        "acme"
    );
    assert!(
        t.get("/api/v1/workspaces/trash", Some(&adm))
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
    let taken = t
        .post(
            "/api/v1/workspaces",
            json!({ "name": "Acme", "slug": "acme" }),
            Some(&ana),
        )
        .await;
    assert_eq!(
        taken.status,
        StatusCode::CONFLICT,
        "slug stays reserved in the trash"
    );

    assert_eq!(
        t.post("/api/v1/workspaces/acme/restore", json!({}), Some(&adm))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        t.post("/api/v1/workspaces/acme/restore", json!({}), Some(&ana))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        t.get("/api/v1/workspaces/acme", Some(&adm)).await.status,
        StatusCode::OK
    );

    // Purge removes only what was trashed before the cutoff.
    assert_eq!(
        t.delete("/api/v1/workspaces/acme", Some(&ana)).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        t.store
            .purge_trashed_workspaces("2000-01-01T00:00:00.000000Z")
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        t.store
            .purge_trashed_workspaces("2999-01-01T00:00:00.000000Z")
            .await
            .unwrap(),
        1
    );
    assert!(
        t.get("/api/v1/workspaces/trash", Some(&ana))
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn role_changes_and_removal_follow_the_matrix() {
    let t = common::app().await;
    let ana = t.setup_admin().await; // owner
    t.workspace(&ana, "acme").await;
    let adm = t.join(&ana, "acme", "adm@example.com", "admin").await;
    let mem = t.join(&ana, "acme", "mem@example.com", "member").await;
    let gus = t.join(&adm, "acme", "gus@example.com", "guest").await;
    let (ana_id, adm_id, mem_id, gus_id) = (
        t.user_id(&ana).await,
        t.user_id(&adm).await,
        t.user_id(&mem).await,
        t.user_id(&gus).await,
    );
    let role = |id: &str| format!("/api/v1/workspaces/acme/members/{id}");

    // Admin: member ↔ guest only (D1).
    assert_eq!(
        t.patch(&role(&gus_id), json!({ "role": "member" }), Some(&adm))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        t.patch(&role(&gus_id), json!({ "role": "admin" }), Some(&adm))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.patch(&role(&ana_id), json!({ "role": "member" }), Some(&adm))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    // Member: nothing.
    assert_eq!(
        t.patch(&role(&gus_id), json!({ "role": "guest" }), Some(&mem))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    // Admins cannot invite admins.
    let r = t
        .post(
            "/api/v1/workspaces/acme/invites",
            json!({ "invites": [{ "email": "x@example.com", "role": "admin" }] }),
            Some(&adm),
        )
        .await;
    assert!(r.body[0]["error"].is_string());
    // Owner: last owner cannot step down; after promoting another owner it can.
    assert_eq!(
        t.patch(&role(&ana_id), json!({ "role": "admin" }), Some(&ana))
            .await
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        t.post("/api/v1/workspaces/acme/leave", json!({}), Some(&ana))
            .await
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        t.patch(&role(&adm_id), json!({ "role": "owner" }), Some(&ana))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        t.patch(&role(&ana_id), json!({ "role": "admin" }), Some(&ana))
            .await
            .status,
        StatusCode::NO_CONTENT
    );

    // Removal: adm is now owner and removes mem; gus leaves on their own.
    assert_eq!(
        t.delete(&role(&mem_id), Some(&adm)).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        t.get("/api/v1/workspaces/acme", Some(&mem)).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        t.post("/api/v1/workspaces/acme/leave", json!({}), Some(&gus))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let members = t
        .get("/api/v1/workspaces/acme/members", Some(&adm))
        .await
        .body;
    assert_eq!(members.as_array().unwrap().len(), 2);
    assert_eq!(members[0]["role"], "owner");
}

#[tokio::test]
async fn invites_existing_users_and_revocation() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    t.workspace(&ana, "acme").await;
    let bob = t.user("bob@example.com").await;
    let other = t.user("other@example.com").await;

    let r = t
        .post(
            "/api/v1/workspaces/acme/invites",
            json!({ "invites": [
        { "email": "Bob@Example.com", "role": "member" },
        { "email": "not-an-email", "role": "member" },
        { "email": "ana@example.com", "role": "member" }
    ] }),
            Some(&ana),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert_eq!(r.body[0]["email"], "bob@example.com");
    assert_eq!(r.body[0]["emailed"], false);
    assert!(r.body[1]["error"].is_string());
    assert_eq!(r.body[2]["error"], "already a member");
    let token = r.body[0]["link"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();

    let info = t.get(&format!("/api/v1/invites/{token}"), None).await;
    assert_eq!(info.body["workspace_name"], "ACME");
    assert_eq!(info.body["account_exists"], true);
    assert_eq!(info.body["signup_allowed"], true);

    // Signup is refused for an existing account; another account cannot accept.
    let s = t
        .post(
            &format!("/api/v1/invites/{token}/signup"),
            json!({ "name": "B", "password": "password123" }),
            None,
        )
        .await;
    assert_eq!(s.status, StatusCode::CONFLICT);
    let a = t
        .post(
            &format!("/api/v1/invites/{token}/accept"),
            json!({}),
            Some(&other),
        )
        .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    let a = t
        .post(
            &format!("/api/v1/invites/{token}/accept"),
            json!({}),
            Some(&bob),
        )
        .await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.body["workspace_slug"], "acme");
    assert_eq!(
        t.get(&format!("/api/v1/invites/{token}"), None)
            .await
            .status,
        StatusCode::NOT_FOUND,
        "single use"
    );

    // Revoked invites stop working; a newer invite replaces an older one.
    let r1 = t
        .post(
            "/api/v1/workspaces/acme/invites",
            json!({ "invites": [{ "email": "c@example.com", "role": "guest" }] }),
            Some(&ana),
        )
        .await;
    let r2 = t
        .post(
            "/api/v1/workspaces/acme/invites",
            json!({ "invites": [{ "email": "c@example.com", "role": "member" }] }),
            Some(&ana),
        )
        .await;
    let tok1 = r1.body[0]["link"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    assert_eq!(
        t.get(&format!("/api/v1/invites/{tok1}"), None).await.status,
        StatusCode::NOT_FOUND
    );
    let pending = t
        .get("/api/v1/workspaces/acme/invites", Some(&ana))
        .await
        .body;
    assert_eq!(pending.as_array().unwrap().len(), 1);
    assert_eq!(pending[0]["role"], "member");
    let id = pending[0]["id"].as_str().unwrap();
    assert_eq!(
        t.delete(&format!("/api/v1/workspaces/acme/invites/{id}"), Some(&ana))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let tok2 = r2.body[0]["link"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    assert_eq!(
        t.get(&format!("/api/v1/invites/{tok2}"), None).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn closed_signup_blocks_new_accounts_from_invites() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    t.workspace(&ana, "acme").await;
    assert_eq!(
        t.patch(
            "/api/v1/admin/settings",
            json!({ "signup_policy": "closed" }),
            Some(&ana)
        )
        .await
        .status,
        StatusCode::OK
    );
    let r = t
        .post(
            "/api/v1/workspaces/acme/invites",
            json!({ "invites": [{ "email": "new@example.com", "role": "member" }] }),
            Some(&ana),
        )
        .await;
    let token = r.body[0]["link"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    assert_eq!(
        t.get(&format!("/api/v1/invites/{token}"), None).await.body["signup_allowed"],
        false
    );
    let s = t
        .post(
            &format!("/api/v1/invites/{token}/signup"),
            json!({ "name": "N", "password": "password123" }),
            None,
        )
        .await;
    assert_eq!(s.status, StatusCode::FORBIDDEN);
}
