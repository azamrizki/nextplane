mod common;

use std::time::Duration;

use axum::http::StatusCode;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};
use yrs::sync::{Message, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, GetString, ReadTxn, StateVector, Text, Transact, Update};

struct W {
    t: common::TestApp,
    owner: String,
    con: String,
    con2: String,
    com: String,
    base: String,
}

async fn world() -> W {
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
    let mut people = Vec::new();
    for (email, role) in [
        ("con@example.com", "contributor"),
        ("con2@example.com", "contributor"),
        ("com@example.com", "commenter"),
    ] {
        let c = t.join(&owner, "acme", email, "member").await;
        let id = t.user_id(&c).await;
        t.post(
            &format!("{base}/members"),
            json!({ "user_id": id, "role": role }),
            Some(&owner),
        )
        .await;
        people.push(c);
    }
    let com = people.pop().unwrap();
    let con2 = people.pop().unwrap();
    let con = people.pop().unwrap();
    W {
        t,
        owner,
        con,
        con2,
        com,
        base,
    }
}

#[tokio::test]
async fn page_permissions_lock_versions_and_search() {
    let w = world().await;
    let pg = format!("{}/pages", w.base);
    let mine =
        w.t.post(&pg, json!({ "title": "Release plan" }), Some(&w.con))
            .await;
    assert_eq!(mine.status, StatusCode::CREATED, "{}", mine.body);
    assert_eq!(mine.body["can_edit"], true);
    let id = mine.body["id"].as_str().unwrap().to_string();
    let url = format!("{pg}/{id}");
    assert_eq!(
        w.t.post(&pg, json!({ "title": "x" }), Some(&w.com))
            .await
            .status,
        StatusCode::FORBIDDEN,
        "commenters read only"
    );

    // Owner saves a snapshot: sanitized, searchable, first version kept.
    let s =
        w.t.patch(
            &url,
            json!({ "html": "<h2>Plan</h2><p>Ship the onboarding revamp<script>x</script></p>" }),
            Some(&w.con),
        )
        .await;
    assert_eq!(s.status, StatusCode::OK, "{}", s.body);
    assert!(!s.body["html"].as_str().unwrap().contains("script"));
    let hits =
        w.t.get("/api/v1/workspaces/acme/search?q=onboarding", Some(&w.com))
            .await
            .body;
    assert_eq!(hits["pages"][0]["title"], "Release plan");
    assert_eq!(
        w.t.get(&format!("{url}/versions"), Some(&w.con))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Others: another contributor reads but cannot edit; the admin can.
    let other = w.t.get(&url, Some(&w.con2)).await.body;
    assert_eq!(other["can_edit"], false);
    assert_eq!(
        w.t.patch(&url, json!({ "title": "Mine now" }), Some(&w.con2))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.t.patch(&url, json!({ "title": "Release plan v2" }), Some(&w.owner))
            .await
            .status,
        StatusCode::OK
    );

    // Lock: nobody edits until unlocked; only owner/admin lock.
    assert_eq!(
        w.t.post(&format!("{url}/do/lock"), json!({}), Some(&w.con2))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.t.post(&format!("{url}/do/lock"), json!({}), Some(&w.con))
            .await
            .body["can_edit"],
        false
    );
    assert_eq!(
        w.t.patch(&url, json!({ "html": "<p>x</p>" }), Some(&w.owner))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    w.t.post(&format!("{url}/do/unlock"), json!({}), Some(&w.owner))
        .await;

    // Private pages are invisible to others.
    let p =
        w.t.post(
            &pg,
            json!({ "title": "Notes", "access": "private" }),
            Some(&w.con),
        )
        .await
        .body;
    let pid = p["id"].as_str().unwrap();
    assert_eq!(
        w.t.get(&format!("{pg}/{pid}"), Some(&w.owner)).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        w.t.get(&pg, Some(&w.owner))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        w.t.patch(
            &format!("{pg}/{pid}"),
            json!({ "access": "project" }),
            Some(&w.owner)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // Nesting cannot loop; archive and delete.
    let child =
        w.t.post(
            &pg,
            json!({ "title": "Child", "parent_id": id }),
            Some(&w.con),
        )
        .await
        .body;
    let cid = child["id"].as_str().unwrap();
    assert_eq!(
        w.t.patch(&url, json!({ "parent_id": cid }), Some(&w.con))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    w.t.post(&format!("{url}/do/archive"), json!({}), Some(&w.con))
        .await;
    assert_eq!(
        w.t.get(&format!("{pg}?archived=true"), Some(&w.con))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        w.t.get("/api/v1/workspaces/acme/search?q=onboarding", Some(&w.com))
            .await
            .body["pages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        w.t.delete(&url, Some(&w.con2)).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.t.delete(&url, Some(&w.con)).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        w.t.get(&url, Some(&w.con)).await.status,
        StatusCode::NOT_FOUND
    );
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(
    addr: &str,
    page: &str,
    cookie: &str,
    origin: &str,
) -> Result<Ws, tungstenite::Error> {
    let mut req = format!("ws://{addr}/api/v1/collab/{page}")
        .into_client_request()
        .unwrap();
    req.headers_mut().insert("cookie", cookie.parse().unwrap());
    req.headers_mut().insert("origin", origin.parse().unwrap());
    let (ws, _) = tokio_tungstenite::connect_async(req).await?;
    Ok(ws)
}

/// Reads frames until the doc contains `want` or time runs out; applies sync messages to `doc`.
async fn pump(ws: &mut Ws, doc: &Doc, want: &str) -> String {
    let text = doc.get_or_insert_text("t");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let got = text.get_string(&doc.transact());
        if got.contains(want) {
            return got;
        }
        let Ok(Some(Ok(msg))) = tokio::time::timeout_at(deadline, ws.next()).await else {
            return got;
        };
        if let tungstenite::Message::Binary(b) = msg
            && let Ok(Message::Sync(SyncMessage::SyncStep2(u) | SyncMessage::Update(u))) =
                Message::decode_v1(&b)
        {
            doc.transact_mut()
                .apply_update(Update::decode_v1(&u).unwrap())
                .unwrap();
        }
    }
}

async fn send_text(ws: &mut Ws, doc: &Doc, s: &str) {
    let text = doc.get_or_insert_text("t");
    let before = doc.transact().state_vector();
    {
        let mut tx = doc.transact_mut();
        let len = text.len(&tx);
        text.insert(&mut tx, len, s);
    }
    let diff = doc.transact().encode_state_as_update_v1(&before);
    ws.send(tungstenite::Message::Binary(
        Message::Sync(SyncMessage::Update(diff)).encode_v1().into(),
    ))
    .await
    .unwrap();
}

async fn ask_state(ws: &mut Ws) {
    let m = Message::Sync(SyncMessage::SyncStep1(StateVector::default())).encode_v1();
    ws.send(tungstenite::Message::Binary(m.into()))
        .await
        .unwrap();
}

#[tokio::test]
async fn two_editors_sync_through_the_server_and_it_is_saved() {
    let w = world().await;
    let page =
        w.t.post(
            &format!("{}/pages", w.base),
            json!({ "title": "Live" }),
            Some(&w.con),
        )
        .await
        .body;
    let id = page["id"].as_str().unwrap().to_string();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let app = w.t.app.clone();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let origin = format!("http://{addr}");

    // Cross-site sockets are refused.
    assert!(
        connect(&addr, &id, &w.con, "http://evil.test")
            .await
            .is_err()
    );

    let mut a = connect(&addr, &id, &w.con, &origin).await.unwrap();
    let mut b = connect(&addr, &id, &w.owner, &origin).await.unwrap();
    let mut r = connect(&addr, &id, &w.com, &origin).await.unwrap();
    let (da, db, dr) = (Doc::new(), Doc::new(), Doc::new());
    send_text(&mut a, &da, "Hello from A. ").await;
    assert!(
        pump(&mut b, &db, "Hello from A.")
            .await
            .contains("Hello from A.")
    );
    send_text(&mut b, &db, "And B.").await;
    assert!(pump(&mut a, &da, "And B.").await.contains("And B."));
    assert!(
        pump(&mut r, &dr, "And B.").await.contains("And B."),
        "readers see changes live"
    );

    // A reader's edits are ignored by the server.
    send_text(&mut r, &dr, " Reader was here.").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut late = connect(&addr, &id, &w.owner, &origin).await.unwrap();
    let dl = Doc::new();
    ask_state(&mut late).await;
    let seen = pump(&mut late, &dl, "And B.").await;
    assert_eq!(seen, "Hello from A. And B.");

    // Everyone leaves: the document is saved.
    for ws in [&mut a, &mut b, &mut r, &mut late] {
        ws.close(None).await.ok();
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let saved = loop {
        if let Some(bytes) = w.t.store.page_ydoc(id.parse().unwrap()).await.unwrap() {
            let d = Doc::new();
            d.transact_mut()
                .apply_update(Update::decode_v1(&bytes).unwrap())
                .unwrap();
            let s = d.get_or_insert_text("t").get_string(&d.transact());
            if !s.is_empty() || tokio::time::Instant::now() > deadline {
                break s;
            }
        }
        if tokio::time::Instant::now() > deadline {
            break String::new();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(saved, "Hello from A. And B.");
    let _: Value = Value::Null;
}

#[tokio::test]
async fn the_server_keeps_html_search_and_versions_of_the_live_document() {
    use yrs::types::xml::{XmlElementPrelim, XmlTextPrelim};
    use yrs::{XmlElementRef, XmlFragment};
    let w = world().await;
    let pg = format!("{}/pages", w.base);
    let page =
        w.t.post(&pg, json!({ "title": "Specs" }), Some(&w.con))
            .await
            .body;
    let id = page["id"].as_str().unwrap().to_string();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let app = w.t.app.clone();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut a = connect(&addr, &id, &w.con, &format!("http://{addr}"))
        .await
        .unwrap();

    // What Tiptap sends: a paragraph element holding text.
    let doc = Doc::new();
    let frag = doc.get_or_insert_xml_fragment("default");
    let before = doc.transact().state_vector();
    {
        let mut tx = doc.transact_mut();
        let p: XmlElementRef = frag.push_back(&mut tx, XmlElementPrelim::empty("paragraph"));
        p.push_back(
            &mut tx,
            XmlTextPrelim::new("Quarterly roadmap for zeppelins"),
        );
    }
    let diff = doc.transact().encode_state_as_update_v1(&before);
    a.send(tungstenite::Message::Binary(
        Message::Sync(SyncMessage::Update(diff)).encode_v1().into(),
    ))
    .await
    .unwrap();
    a.close(None).await.ok();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let html = loop {
        let got = w.t.get(&format!("{pg}/{id}"), Some(&w.con)).await.body;
        let h = got["html"].as_str().unwrap_or("").to_string();
        if !h.is_empty() || tokio::time::Instant::now() > deadline {
            assert_eq!(got["has_doc"], true);
            break h;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(html, "<p>Quarterly roadmap for zeppelins</p>");
    let hits =
        w.t.get("/api/v1/workspaces/acme/search?q=zeppelins", Some(&w.com))
            .await
            .body;
    assert_eq!(hits["pages"][0]["title"], "Specs");
    assert_eq!(
        w.t.get(&format!("{pg}/{id}/versions"), Some(&w.con))
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1,
        "first save keeps a version"
    );
    // The text now changes only through the live document.
    assert_eq!(
        w.t.patch(
            &format!("{pg}/{id}"),
            json!({ "html": "<p>x</p>" }),
            Some(&w.con)
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    // A manual version.
    assert_eq!(
        w.t.post(&format!("{pg}/{id}/versions"), json!({}), Some(&w.con))
            .await
            .status,
        StatusCode::CREATED
    );
    assert_eq!(
        w.t.post(&format!("{pg}/{id}/versions"), json!({}), Some(&w.com))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn removing_someone_from_the_project_closes_their_page_socket() {
    let w = world().await;
    let page =
        w.t.post(
            &format!("{}/pages", w.base),
            json!({ "title": "Roadmap" }),
            Some(&w.con),
        )
        .await
        .body;
    let id = page["id"].as_str().unwrap().to_string();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let app = w.t.app.clone();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let origin = format!("http://{addr}");
    let mut r = connect(&addr, &id, &w.com, &origin).await.unwrap();
    let _ = tokio::time::timeout(Duration::from_millis(300), r.next()).await; // hello

    let uid = w.t.user_id(&w.com).await;
    assert_eq!(
        w.t.delete(&format!("{}/members/{uid}", w.base), Some(&w.owner))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let closed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match r.next().await {
                None | Some(Err(_)) | Some(Ok(tungstenite::Message::Close(_))) => return true,
                Some(Ok(_)) => continue,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(closed, "the socket is closed after removal");
    assert!(
        connect(&addr, &id, &w.com, &origin).await.is_err(),
        "and cannot reconnect"
    );
}

#[tokio::test]
async fn duplicate_move_and_live_references() {
    let w = world().await;
    let pg = format!("{}/pages", w.base);
    let a =
        w.t.post(&pg, json!({ "title": "Alpha" }), Some(&w.con))
            .await
            .body;
    let b =
        w.t.post(&pg, json!({ "title": "Beta" }), Some(&w.con))
            .await
            .body;
    let aid = a["id"].as_str().unwrap();
    w.t.patch(
        &format!("{pg}/{aid}"),
        json!({ "html": "<p>Original text</p>" }),
        Some(&w.con),
    )
    .await;

    // Duplicate: next to the original, same text, owned by whoever copied it.
    let copy =
        w.t.post(&format!("{pg}/{aid}/duplicate"), json!({}), Some(&w.owner))
            .await;
    assert_eq!(copy.status, StatusCode::CREATED, "{}", copy.body);
    assert_eq!(copy.body["title"], "Alpha (copy)");
    let order: Vec<String> =
        w.t.get(&pg, Some(&w.con))
            .await
            .body
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["title"].as_str().unwrap().to_string())
            .collect();
    assert_eq!(order, ["Alpha", "Alpha (copy)", "Beta"]);
    let cid = copy.body["id"].as_str().unwrap();
    assert_eq!(
        w.t.get(&format!("{pg}/{cid}"), Some(&w.con)).await.body["html"],
        "<p>Original text</p>"
    );
    assert_eq!(
        w.t.post(&format!("{pg}/{aid}/duplicate"), json!({}), Some(&w.com))
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    // Move Beta under Alpha, then to the top level before Alpha.
    let bid = b["id"].as_str().unwrap();
    let moved =
        w.t.patch(
            &format!("{pg}/{bid}"),
            json!({ "parent_id": aid }),
            Some(&w.con),
        )
        .await
        .body;
    assert_eq!(moved["parent_id"], aid);
    let top =
        w.t.patch(
            &format!("{pg}/{bid}"),
            json!({ "parent_id": null, "sort_order": a["sort_order"].as_f64().unwrap() - 1.0 }),
            Some(&w.con),
        )
        .await
        .body;
    assert!(top["parent_id"].is_null());
    assert_eq!(w.t.get(&pg, Some(&w.con)).await.body[0]["title"], "Beta");
    assert_eq!(
        w.t.patch(
            &format!("{pg}/{bid}"),
            json!({ "parent_id": cid }),
            Some(&w.con2)
        )
        .await
        .status,
        StatusCode::FORBIDDEN,
        "others' pages stay put"
    );

    // A key resolves to a card with title and state for anyone who can read the project.
    w.t.post(
        &format!("{}/work-items", w.base),
        json!({ "title": "Fix the login" }),
        Some(&w.con),
    )
    .await;
    let r =
        w.t.get(
            "/api/v1/workspaces/acme/work-items/by-key/web-1",
            Some(&w.com),
        )
        .await
        .body;
    assert_eq!(
        (r["key"].as_str(), r["title"].as_str()),
        (Some("WEB-1"), Some("Fix the login"))
    );
    assert_eq!(r["state"]["group"], "unstarted");
}
