mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use time::{Duration, OffsetDateTime};

fn day(offset: i64) -> String {
    let d = OffsetDateTime::now_utc().date() + Duration::days(offset);
    format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
}

#[tokio::test]
async fn settings_are_checked_and_the_daily_pass_archives_and_closes() {
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
    let states = t.get(&format!("{base}/states"), Some(&owner)).await.body;
    let state = |g: &str| {
        states
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["group"] == g)
            .unwrap()["id"]
            .clone()
    };

    // Settings: project admins only, 0 to 12 months, a cancelled state of the project.
    assert_eq!(
        t.patch(&base, json!({ "archive_in": 1 }), Some(&contrib))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.patch(&base, json!({ "archive_in": 13 }), Some(&owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.patch(
            &base,
            json!({ "close_in": 3, "close_state_id": state("completed") }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let r = t
        .patch(&base, json!({ "archive_in": 1, "close_in": 3 }), Some(&owner))
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["archive_in"], 1);
    assert_eq!(r.body["close_state_id"], state("cancelled"), "the first cancelled state");

    let items = format!("{base}/work-items");
    let make = |body: Value| {
        let t = &t;
        let items = items.clone();
        let owner = owner.clone();
        async move {
            t.post(&items, body, Some(&owner)).await.body["id"]
                .as_str()
                .unwrap()
                .to_string()
        }
    };
    let cycle = t
        .post(
            &format!("{base}/cycles"),
            json!({ "name": "Long", "start_date": day(-1), "end_date": day(60) }),
            Some(&owner),
        )
        .await
        .body["id"]
        .clone();
    let module = t
        .post(
            &format!("{base}/modules"),
            json!({ "name": "Open ended" }),
            Some(&owner),
        )
        .await
        .body["id"]
        .clone();
    let done = make(json!({ "title": "Done", "state_id": state("completed") })).await;
    let stale = make(json!({ "title": "Stale" })).await;
    let in_cycle = make(json!({ "title": "In cycle", "state_id": state("completed"), "cycle_id": cycle })).await;
    let in_module = make(json!({ "title": "In module", "module_ids": [module] })).await;

    let mut rx = t.events.subscribe();
    let now = OffsetDateTime::now_utc();
    // A month later: the closed item is archived; the one in a running cycle waits.
    let r = np_api::automations::run(&t.store, &t.events, now + Duration::days(31))
        .await
        .unwrap();
    assert_eq!((r.archived, r.closed), (1, 0));
    let ev = rx.try_recv().unwrap();
    assert_eq!((ev.area.as_str(), ev.action.as_str()), ("work_item", "archived"));
    let one = t.get(&format!("{items}/{done}"), Some(&owner)).await.body;
    assert!(one["archived_at"].is_string());
    let act = t
        .get(&format!("{items}/{done}/activity"), Some(&owner))
        .await
        .body;
    let last = act.as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["verb"], "archived");
    let bot = t
        .store
        .user(last["actor_id"].as_str().unwrap().parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!((bot.name.as_str(), bot.is_bot), ("NextPlane", true));

    // Three months on: the untouched open item is closed into the cancelled state; the cycle
    // has ended, so its item is archived; the item in an open-ended module stays.
    let r = np_api::automations::run(&t.store, &t.events, now + Duration::days(95))
        .await
        .unwrap();
    assert_eq!((r.archived, r.closed), (1, 1));
    let s = t.get(&format!("{items}/{stale}"), Some(&owner)).await.body;
    assert_eq!(s["state_id"], state("cancelled"));
    assert!(s["archived_at"].is_null());
    let c = t.get(&format!("{items}/{in_cycle}"), Some(&owner)).await.body;
    assert!(c["archived_at"].is_string());
    let m = t.get(&format!("{items}/{in_module}"), Some(&owner)).await.body;
    assert_ne!(m["state_id"], state("cancelled"), "left alone");

    // Turning auto-close off forgets the state; nothing runs for the project any more.
    let r = t
        .patch(&base, json!({ "close_in": 0, "archive_in": 0 }), Some(&owner))
        .await;
    assert!(r.body["close_state_id"].is_null());
    let r = np_api::automations::run(&t.store, &t.events, now + Duration::days(400))
        .await
        .unwrap();
    assert_eq!((r.archived, r.closed), (0, 0));
}
