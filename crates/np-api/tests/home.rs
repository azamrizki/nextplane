mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

const H: &str = "/api/v1/workspaces/acme";

async fn world() -> (common::TestApp, String, String, String) {
    let t = common::app().await;
    let owner = t.setup_admin().await;
    t.workspace(&owner, "acme").await;
    let p = t
        .post(
            &format!("{H}/projects"),
            json!({ "name": "Website" }),
            Some(&owner),
        )
        .await
        .body;
    let bob = t.join(&owner, "acme", "bob@example.com", "member").await;
    (t, owner, bob, p["id"].as_str().unwrap().to_string())
}

fn len(v: &Value) -> usize {
    v.as_array().unwrap().len()
}

#[tokio::test]
async fn widgets_and_quick_links_are_personal() {
    let (t, owner, bob, _) = world().await;
    let w = t.get(&format!("{H}/home/widgets"), Some(&owner)).await.body;
    let keys: Vec<&str> = w
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["quick_links", "recents", "stickies"]);
    let w = t
        .patch(
            &format!("{H}/home/widgets/stickies"),
            json!({ "is_enabled": false, "sort_order": 99999 }),
            Some(&owner),
        )
        .await
        .body;
    assert_eq!(w[0]["key"], "stickies");
    assert_eq!(w[0]["is_enabled"], false);
    assert_eq!(
        t.patch(
            &format!("{H}/home/widgets/weather"),
            json!({ "is_enabled": false }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    // Someone else's home is untouched.
    let w = t.get(&format!("{H}/home/widgets"), Some(&bob)).await.body;
    assert_eq!(w[2]["key"], "stickies");
    assert_eq!(w[2]["is_enabled"], true);

    let links = format!("{H}/quick-links");
    let l = t
        .post(
            &links,
            json!({ "url": "example.com/docs", "title": " Docs " }),
            Some(&owner),
        )
        .await;
    assert_eq!(l.status, StatusCode::CREATED, "{}", l.body);
    assert_eq!(l.body["url"], "http://example.com/docs");
    assert_eq!(l.body["title"], "Docs");
    assert_eq!(
        t.post(
            &links,
            json!({ "url": "http://example.com/docs" }),
            Some(&owner)
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    for bad in ["", "not a url", "javascript:alert(1)", "ftp://x.io"] {
        assert_eq!(
            t.post(&links, json!({ "url": bad }), Some(&owner))
                .await
                .status,
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
    let id = l.body["id"].as_str().unwrap();
    assert_eq!(len(&t.get(&links, Some(&bob)).await.body), 0);
    assert_eq!(
        t.patch(
            &format!("{links}/{id}"),
            json!({ "title": "x" }),
            Some(&bob)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let r = t
        .patch(
            &format!("{links}/{id}"),
            json!({ "url": "https://example.com" }),
            Some(&owner),
        )
        .await;
    assert_eq!(r.body["url"], "https://example.com");
    assert_eq!(
        t.delete(&format!("{links}/{id}"), Some(&owner))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(len(&t.get(&links, Some(&owner)).await.body), 0);
}

#[tokio::test]
async fn recents_show_what_you_may_still_see_newest_first() {
    let (t, owner, bob, pid) = world().await;
    let base = format!("{H}/projects/{pid}");
    let item = t
        .post(
            &format!("{base}/work-items"),
            json!({ "title": "Fix login" }),
            Some(&owner),
        )
        .await
        .body;
    let page = t
        .post(
            &format!("{base}/pages"),
            json!({ "title": "Roadmap" }),
            Some(&owner),
        )
        .await
        .body;
    let recents = format!("{H}/recents");
    let visit = |entity: &str, id: &Value| json!({ "entity": entity, "id": id, "project_id": pid });
    for (e, id) in [
        ("project", json!(pid)),
        ("work_item", item["id"].clone()),
        ("page", page["id"].clone()),
    ] {
        let r = t.post(&recents, visit(e, &id), Some(&owner)).await;
        assert_eq!(r.status, StatusCode::NO_CONTENT, "{e}: {}", r.body);
    }
    let r = t.get(&recents, Some(&owner)).await.body;
    let kinds: Vec<&str> = r
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["entity"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["page", "work_item", "project"]);
    assert_eq!(r[1]["title"], "Fix login");
    assert_eq!(r[1]["identifier"], "WEB");
    assert_eq!(r[1]["state"]["group"], "unstarted");
    assert_eq!(r[2]["name"], "Website");
    let only = t
        .get(&format!("{recents}?entity=work_item"), Some(&owner))
        .await
        .body;
    assert_eq!(len(&only), 1);
    // A revisit moves it to the top instead of adding a row.
    t.post(&recents, visit("project", &json!(pid)), Some(&owner))
        .await;
    let r = t.get(&recents, Some(&owner)).await.body;
    assert_eq!((len(&r), r[0]["entity"].as_str()), (3, Some("project")));

    // You cannot record what you may not see, and what you lose access to drops out.
    assert_eq!(
        t.post(&recents, visit("work_item", &item["id"]), Some(&bob))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    t.patch(&base, json!({ "feature_pages": false }), Some(&owner))
        .await;
    let r = t.get(&recents, Some(&owner)).await.body;
    assert_eq!(len(&r), 2, "pages are off: {r}");

    // Only the newest 20 are kept.
    for i in 0..25 {
        let w = t
            .post(
                &format!("{base}/work-items"),
                json!({ "title": format!("Item {i}") }),
                Some(&owner),
            )
            .await
            .body;
        t.post(&recents, visit("work_item", &w["id"]), Some(&owner))
            .await;
    }
    let r = t.get(&recents, Some(&owner)).await.body;
    assert_eq!(len(&r), 20);
    assert_eq!(r[0]["title"], "Item 24");
}

#[tokio::test]
async fn stickies_are_private_coloured_and_searchable() {
    let (t, owner, bob, _) = world().await;
    let mut events = t.events.subscribe();
    let s = format!("{H}/stickies");
    let a = t
        .post(
            &s,
            json!({ "description_html": "<p>Buy <strong>coffee</strong></p><script>x</script>" }),
            Some(&owner),
        )
        .await;
    assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
    assert!(
        [
            "gray",
            "peach",
            "pink",
            "orange",
            "green",
            "light-blue",
            "dark-blue",
            "purple"
        ]
        .contains(&a.body["color"].as_str().unwrap())
    );
    assert!(
        !a.body["description_html"]
            .as_str()
            .unwrap()
            .contains("script")
    );
    let b = t
        .post(
            &s,
            json!({ "description_html": "<p>Call the bank</p>", "color": "green" }),
            Some(&owner),
        )
        .await
        .body;
    assert_eq!(b["color"], "green");
    assert!(
        b["sort_order"].as_f64() > a.body["sort_order"].as_f64(),
        "new ones go on top"
    );
    assert_eq!(
        t.post(&s, json!({ "color": "neon" }), Some(&owner))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    let all = t.get(&s, Some(&owner)).await.body;
    assert_eq!(all[0]["id"], b["id"]);
    let found = t.get(&format!("{s}?q=COFFEE"), Some(&owner)).await.body;
    assert_eq!(len(&found), 1);
    assert_eq!(len(&t.get(&s, Some(&bob)).await.body), 0);
    let id = a.body["id"].as_str().unwrap();
    assert_eq!(
        t.patch(&format!("{s}/{id}"), json!({ "color": "pink" }), Some(&bob))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let r = t
        .patch(
            &format!("{s}/{id}"),
            json!({ "description_html": "<p>Buy tea</p>", "color": "pink" }),
            Some(&owner),
        )
        .await
        .body;
    assert_eq!(r["color"], "pink");
    assert_eq!(
        len(&t.get(&format!("{s}?q=coffee"), Some(&owner)).await.body),
        0
    );
    assert_eq!(
        len(&t.get(&format!("{s}?q=tea"), Some(&owner)).await.body),
        1
    );
    assert_eq!(
        t.delete(&format!("{s}/{id}"), Some(&owner)).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(len(&t.get(&s, Some(&owner)).await.body), 1);
    // Personal things are not announced to the workspace (realtime, webhooks).
    assert!(events.try_recv().is_err());
}
