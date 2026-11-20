//! Webhooks (SPRINT S3.5): every change event of a workspace is queued for its matching
//! webhooks and POSTed as JSON, signed with HMAC-SHA256 of the body
//! (`X-NextPlane-Signature: sha256=<hex>`). Failed deliveries are retried after 1 min,
//! 5 min, 30 min, 2 h and 6 h, then marked failed.

use std::sync::Arc;
use std::time::Duration;

use np_store::Store;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

use crate::events::Event;

/// HMAC-SHA256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.map(|b| b ^ 0x36));
    inner.update(msg);
    let mut outer = Sha256::new();
    outer.update(k.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time comparison of two signatures.
pub fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Webhook areas people can subscribe to.
pub const AREAS: &[&str] = &[
    "work_item",
    "comment",
    "attachment",
    "link",
    "relation",
    "reaction",
    "cycle",
    "module",
    "view",
    "page",
    "intake",
    "project",
    "state",
    "label",
    "member",
];

const RETRY_MINUTES: &[i64] = &[1, 5, 30, 120, 360];

/// Starts the dispatcher: it turns events into queued deliveries and sends what is due.
pub fn spawn(
    store: Store,
    events: crate::events::Bus,
    http: reqwest::Client,
    base: Option<String>,
    version: String,
) {
    let wake = Arc::new(Notify::new());
    {
        let (store, wake) = (store.clone(), wake.clone());
        let mut rx = events.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(e) => {
                        if let Err(err) = enqueue(&store, &e, base.as_deref()).await {
                            tracing::warn!(error = %err, "webhook enqueue failed");
                        }
                        wake.notify_one();
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(missed = n, "webhook dispatcher fell behind")
                    }
                    Err(_) => return,
                }
            }
        });
    }
    tokio::spawn(async move {
        loop {
            if let Err(err) = send_due(&store, &http, &version).await {
                tracing::warn!(error = %err, "webhook delivery failed");
            }
            tokio::select! {
                _ = wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
            }
        }
    });
}

/// The JSON body of an event for webhooks: what changed, and the work item as it is now.
pub async fn payload(store: &Store, e: &Event, base: Option<&str>) -> Value {
    let mut item = Value::Null;
    if let (Some(pid), Some(wid)) = (e.project_id, e.work_item_id)
        && let (Ok(Some(w)), Ok(Some(ident))) = (
            store.work_item(pid, wid).await,
            store.project_identifier(pid).await,
        )
    {
        let state = store
            .states(pid)
            .await
            .ok()
            .and_then(|s| s.into_iter().find(|s| s.id == w.state_id));
        let url = base.map(|b| {
            format!(
                "{}/{}/projects/{pid}/work-items/{wid}",
                b.trim_end_matches('/'),
                e.workspace
            )
        });
        item = json!({
            "id": w.id,
            "key": format!("{ident}-{}", w.sequence),
            "title": w.title,
            "state": state.map(|s| json!({ "name": s.name, "group": s.group })),
            "priority": w.priority,
            "assignee_ids": w.assignee_ids,
            "label_ids": w.label_ids,
            "start_date": w.start_date,
            "due_date": w.due_date,
            "in_intake": w.in_intake,
            "url": url,
        });
    }
    json!({
        "event": format!("{}.{}", e.area, e.action),
        "workspace": e.workspace,
        "project_id": e.project_id,
        "work_item_id": e.work_item_id,
        "actor_id": e.actor,
        "at": np_store::now_ts(),
        "work_item": item,
    })
}

async fn enqueue(store: &Store, e: &Event, base: Option<&str>) -> anyhow::Result<()> {
    if !AREAS.contains(&e.area.as_str()) {
        return Ok(());
    }
    let hooks = store
        .webhooks_for(&e.workspace, &e.area)
        .await
        .map_err(|err| anyhow::anyhow!(err))?;
    if hooks.is_empty() {
        return Ok(());
    }
    let body = payload(store, e, base).await;
    let event = format!("{}.{}", e.area, e.action);
    for (hook, _) in hooks {
        store
            .enqueue_delivery(hook, &event, &body.to_string())
            .await
            .map_err(|err| anyhow::anyhow!(err))?;
    }
    Ok(())
}

async fn send_due(store: &Store, http: &reqwest::Client, version: &str) -> anyhow::Result<()> {
    loop {
        let due = store
            .due_deliveries(20)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        if due.is_empty() {
            return Ok(());
        }
        for d in due {
            // The delivery id goes into the body too, so receivers can drop duplicates.
            let mut body: Value = serde_json::from_str(&d.payload).unwrap_or(Value::Null);
            body["delivery_id"] = json!(d.id);
            let bytes = body.to_string();
            let sig = format!(
                "sha256={}",
                hex(&hmac_sha256(d.secret.as_bytes(), bytes.as_bytes()))
            );
            let res = http
                .post(&d.url)
                .timeout(Duration::from_secs(10))
                .header("content-type", "application/json")
                .header("user-agent", format!("NextPlane-Webhooks/{version}"))
                .header("x-nextplane-event", &d.event)
                .header("x-nextplane-delivery", d.id.to_string())
                .header("x-nextplane-signature", sig)
                .body(bytes)
                .send()
                .await;
            let (ok, code, err) = match res {
                Ok(r) => (
                    r.status().is_success(),
                    Some(r.status().as_u16() as i64),
                    (!r.status().is_success()).then(|| format!("HTTP {}", r.status())),
                ),
                Err(e) => (false, None, Some(short_error(&e))),
            };
            let next = (!ok)
                .then(|| {
                    RETRY_MINUTES.get(d.attempts as usize).map(|m| {
                        np_store::fmt_ts(
                            time::OffsetDateTime::now_utc() + time::Duration::minutes(*m),
                        )
                    })
                })
                .flatten();
            store
                .record_attempt(d.id, ok, code, err.as_deref(), next.as_deref())
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
        }
    }
}

fn short_error(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "timed out"
    } else if e.is_connect() {
        "could not connect"
    } else if e.is_redirect() {
        "too many redirects"
    } else {
        "request failed"
    };
    kind.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_rfc4231() {
        // Test case 2.
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        // Test case 6 (a key longer than the block).
        assert_eq!(
            hex(&hmac_sha256(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
        assert!(same(b"abc", b"abc") && !same(b"abc", b"abd") && !same(b"ab", b"abc"));
    }
}
