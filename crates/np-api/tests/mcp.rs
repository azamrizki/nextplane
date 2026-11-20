mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};

async fn rpc(t: &common::TestApp, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut req = Request::post("/mcp").header(header::CONTENT_TYPE, "application/json");
    if let Some(tk) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {tk}"));
    }
    let r = t
        .send(req.body(Body::from(body.to_string())).unwrap())
        .await;
    (r.status, r.body)
}

async fn call(t: &common::TestApp, token: &str, name: &str, args: Value) -> Value {
    let (_, v) = rpc(t, Some(token), json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } })).await;
    v["result"].clone()
}

fn text(r: &Value) -> Value {
    serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap_or(Value::Null)
}

#[tokio::test]
async fn an_agent_works_through_mcp() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    t.workspace(&ana, "acme").await;
    let p = t
        .post(
            "/api/v1/workspaces/acme/projects",
            json!({ "name": "Website" }),
            Some(&ana),
        )
        .await
        .body;
    let base = format!(
        "/api/v1/workspaces/acme/projects/{}",
        p["id"].as_str().unwrap()
    );
    t.post(
        &format!("{base}/labels"),
        json!({ "name": "Bug", "color": "#e5484d" }),
        Some(&ana),
    )
    .await;
    let bot = t
        .post(
            "/api/v1/workspaces/acme/bots",
            json!({ "name": "Agent" }),
            Some(&ana),
        )
        .await
        .body;
    let token = bot["token"].as_str().unwrap().to_string();
    let bot_id = bot["bot"]["id"].as_str().unwrap().to_string();
    t.post(
        &format!("{base}/members"),
        json!({ "user_id": bot_id, "role": "contributor" }),
        Some(&ana),
    )
    .await;

    // Handshake.
    let (s, init) = rpc(&t, Some(&token), json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } } })).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    let (s, _) = rpc(
        &t,
        Some(&token),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let (_, list) = rpc(
        &t,
        Some(&token),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    )
    .await;
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"create_work_item") && names.contains(&"add_comment"));

    // Work.
    let ws = text(&call(&t, &token, "list_workspaces", json!({})).await);
    assert_eq!(ws[0]["slug"], "acme");
    let created = call(&t, &token, "create_work_item", json!({
        "workspace": "acme", "project": "web", "title": "Fix 500 on checkout",
        "description": "Stack trace:\n<script>x</script>", "priority": "high", "assignees": ["me"], "labels": ["bug"]
    })).await;
    assert_eq!(created["isError"], false, "{created}");
    let item = text(&created);
    assert_eq!(item["key"], "WEB-1");
    assert_eq!(item["assignees"][0], "Agent");
    assert_eq!(item["labels"][0], "Bug");

    let moved = text(
        &call(
            &t,
            &token,
            "update_work_item",
            json!({ "workspace": "acme", "key": "WEB-1", "state": "in progress" }),
        )
        .await,
    );
    assert_eq!(moved["state"], "In Progress");
    let bad = call(
        &t,
        &token,
        "update_work_item",
        json!({ "workspace": "acme", "key": "WEB-1", "state": "Shipping" }),
    )
    .await;
    assert_eq!(bad["isError"], true);
    assert!(
        bad["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Backlog"),
        "lists the real states"
    );

    call(
        &t,
        &token,
        "add_comment",
        json!({ "workspace": "acme", "key": "WEB-1", "text": "Fixed in abc123." }),
    )
    .await;
    let full = text(
        &call(
            &t,
            &token,
            "get_work_item",
            json!({ "workspace": "acme", "key": "web-1" }),
        )
        .await,
    );
    assert!(
        full["description"]
            .as_str()
            .unwrap()
            .contains("Stack trace")
    );
    // Text comes back as text; in the stored HTML it is escaped and cannot run.
    let wi = t.get(&format!("{base}/work-items"), Some(&ana)).await.body[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let html = t
        .get(&format!("{base}/work-items/{wi}"), Some(&ana))
        .await
        .body["description_html"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        html.contains("&lt;script&gt;") && !html.contains("<script>"),
        "{html}"
    );
    assert_eq!(full["comments"][0]["author"], "Agent");
    assert_eq!(full["comments"][0]["text"], "Fixed in abc123.");
    let found = text(
        &call(
            &t,
            &token,
            "search",
            json!({ "workspace": "acme", "query": "checkout" }),
        )
        .await,
    );
    assert_eq!(found[0]["key"], "WEB-1");
    let mine = text(&call(&t, &token, "list_work_items", json!({ "workspace": "acme", "project": "WEB", "assignee": "me", "state_group": "started" })).await);
    assert_eq!(mine.as_array().unwrap().len(), 1);

    // Pages: an agent writes a page, lists it, reads it back and finds it.
    let pg = call(&t, &token, "create_page", json!({ "workspace": "acme", "project": "WEB", "title": "Checkout notes", "text": "First idea.\n\nSecond <b>idea</b>." })).await;
    assert_eq!(pg["isError"], false, "{pg}");
    let pages = text(
        &call(
            &t,
            &token,
            "list_pages",
            json!({ "workspace": "acme", "project": "WEB" }),
        )
        .await,
    );
    assert_eq!(pages[0]["title"], "Checkout notes");
    let one = text(
        &call(
            &t,
            &token,
            "get_page",
            json!({ "workspace": "acme", "project": "WEB", "page": pages[0]["page"] }),
        )
        .await,
    );
    assert!(
        one["text"]
            .as_str()
            .unwrap()
            .contains("Second <b>idea</b>."),
        "{one}"
    );
    let found = text(
        &call(
            &t,
            &token,
            "search",
            json!({ "workspace": "acme", "query": "idea" }),
        )
        .await,
    );
    assert_eq!(found[0]["title"], "Checkout notes");

    // The item's activity names the bot.
    let act = t
        .get(
            &format!(
                "{base}/work-items/{}/activity",
                t.get(&format!("{base}/work-items"), Some(&ana)).await.body[0]["id"]
                    .as_str()
                    .unwrap()
            ),
            Some(&ana),
        )
        .await
        .body;
    assert!(
        act.as_array()
            .unwrap()
            .iter()
            .all(|a| a["actor_id"] == bot_id)
    );

    // Unknown method and tool.
    let (_, e) = rpc(
        &t,
        Some(&token),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/list" }),
    )
    .await;
    assert_eq!(e["error"]["code"], -32601);
    assert_eq!(
        call(&t, &token, "drop_database", json!({})).await["isError"],
        true
    );
}

#[tokio::test]
async fn mcp_permissions_follow_the_token() {
    let t = common::app().await;
    let ana = t.setup_admin().await;
    t.workspace(&ana, "acme").await;
    t.post(
        "/api/v1/workspaces/acme/projects",
        json!({ "name": "Website" }),
        Some(&ana),
    )
    .await;
    let read = t
        .post(
            "/api/v1/me/tokens",
            json!({ "name": "r", "scope": "read" }),
            Some(&ana),
        )
        .await
        .body["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        text(&call(&t, &read, "list_projects", json!({ "workspace": "acme" })).await)[0]["identifier"],
        "WEB"
    );
    let no = call(
        &t,
        &read,
        "create_work_item",
        json!({ "workspace": "acme", "project": "WEB", "title": "x" }),
    )
    .await;
    assert_eq!(no["isError"], true);
    assert!(no["content"][0]["text"].as_str().unwrap().contains("read"));

    // No credentials: tools fail with the API's 401 message.
    let anon = rpc(&t, None, json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "list_workspaces", "arguments": {} } })).await.1;
    assert_eq!(anon["result"]["isError"], true);
    // Bad JSON.
    let r = t
        .send(Request::post("/mcp").body(Body::from("{oops")).unwrap())
        .await;
    assert_eq!(r.body["error"]["code"], -32700);
}
