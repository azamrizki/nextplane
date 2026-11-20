//! Public boards, webhooks and the GitHub connection (SPRINT S3.5).

use np_core::id::{Uuid, new_id};
use np_core::{Error, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::{Store, db_err, now_ts};

// ---- public boards ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PublicItem {
    pub sequence: i64,
    pub title: String,
    #[serde(serialize_with = "crate::integrations::hex_id")]
    pub state_id: Uuid,
    pub priority: String,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub estimate: Option<String>,
    /// Label ids, comma-separated hex.
    #[serde(skip)]
    pub label_hex: Option<String>,
    #[sqlx(skip)]
    pub label_ids: Vec<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_html: Option<String>,
}

pub fn hex_id<S: serde::Serializer>(id: &Uuid, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&id.to_string())
}

/// (workspace name, project id, project name, project identifier) of a published board.
pub type BoardHome = (String, Uuid, String, String);

impl Store {
    pub async fn set_public_board(&self, project: Uuid, token: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE projects SET public_board_token = ? WHERE id = ?")
            .bind(token)
            .bind(project)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn public_board_token(&self, project: Uuid) -> Result<Option<String>> {
        let t: Option<Option<String>> =
            sqlx::query_scalar("SELECT public_board_token FROM projects WHERE id = ?")
                .bind(project)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err)?;
        Ok(t.flatten())
    }

    pub async fn public_board(&self, token: &str) -> Result<Option<BoardHome>> {
        sqlx::query_as(
            "SELECT w.name, p.id, p.name, p.identifier FROM projects p JOIN workspaces w ON w.id = p.workspace_id
             WHERE p.public_board_token = ? AND p.deleted_at IS NULL AND w.deleted_at IS NULL",
        )
        .bind(token)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    /// Open board items (not archived, not drafts or intake), at most `limit`.
    pub async fn public_items(&self, project: Uuid, limit: i64) -> Result<Vec<PublicItem>> {
        let mut rows: Vec<PublicItem> = sqlx::query_as(
            "SELECT w.sequence, w.title, w.state_id, w.priority, w.start_date, w.due_date, w.estimate,
                    (SELECT group_concat(lower(hex(wl.label_id))) FROM work_item_labels wl WHERE wl.work_item_id = w.id) AS label_hex
             FROM work_items w WHERE w.project_id = ? AND w.deleted_at IS NULL AND w.is_draft = 0 AND w.archived_at IS NULL
             ORDER BY w.sort_order, w.sequence LIMIT ?",
        )
        .bind(project)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        for r in &mut rows {
            r.label_ids = r
                .label_hex
                .take()
                .map(|h| h.split(',').filter_map(hex_uuid).collect())
                .unwrap_or_default();
        }
        Ok(rows)
    }

    pub async fn public_item(&self, project: Uuid, sequence: i64) -> Result<Option<PublicItem>> {
        let mut row: Option<PublicItem> = sqlx::query_as(
            "SELECT w.sequence, w.title, w.state_id, w.priority, w.start_date, w.due_date, w.estimate, w.description_html,
                    (SELECT group_concat(lower(hex(wl.label_id))) FROM work_item_labels wl WHERE wl.work_item_id = w.id) AS label_hex
             FROM work_items w WHERE w.project_id = ? AND w.sequence = ? AND w.deleted_at IS NULL AND w.is_draft = 0 AND w.archived_at IS NULL",
        )
        .bind(project)
        .bind(sequence)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        if let Some(r) = &mut row {
            r.label_ids = r
                .label_hex
                .take()
                .map(|h| h.split(',').filter_map(hex_uuid).collect())
                .unwrap_or_default();
        }
        Ok(row)
    }
}

/// A 32-hex-digit id as the usual hyphenated form.
fn hex_uuid(h: &str) -> Option<String> {
    Uuid::parse_str(h).ok().map(|u| u.to_string())
}

// ---- webhooks ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Webhook {
    pub id: Uuid,
    pub url: String,
    #[serde(skip)]
    pub secret: String,
    pub events: String,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Delivery {
    pub id: Uuid,
    pub event: String,
    pub status: String,
    pub attempts: i64,
    pub response_code: Option<i64>,
    pub error: Option<String>,
    pub created_at: String,
    pub last_attempt_at: Option<String>,
    pub next_attempt_at: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

/// A delivery to make now.
#[derive(Debug, Clone, FromRow)]
pub struct DueDelivery {
    pub id: Uuid,
    pub url: String,
    pub secret: String,
    pub event: String,
    pub payload: String,
    pub attempts: i64,
}

const HOOK_COLS: &str = "id, url, secret, events, active, created_at, updated_at";

impl Store {
    pub async fn create_webhook(
        &self,
        ws: Uuid,
        url: &str,
        secret: &str,
        events: &str,
        by: Uuid,
    ) -> Result<Webhook> {
        let id = new_id();
        let now = now_ts();
        sqlx::query("INSERT INTO webhooks (id, workspace_id, url, secret, events, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(id)
            .bind(ws)
            .bind(url)
            .bind(secret)
            .bind(events)
            .bind(by)
            .bind(&now)
            .bind(&now)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        self.webhook(ws, id)
            .await?
            .ok_or_else(|| Error::not_found("webhook not found"))
    }

    pub async fn webhooks(&self, ws: Uuid) -> Result<Vec<Webhook>> {
        sqlx::query_as(&format!(
            "SELECT {HOOK_COLS} FROM webhooks WHERE workspace_id = ? ORDER BY created_at"
        ))
        .bind(ws)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn webhook(&self, ws: Uuid, id: Uuid) -> Result<Option<Webhook>> {
        sqlx::query_as(&format!(
            "SELECT {HOOK_COLS} FROM webhooks WHERE workspace_id = ? AND id = ?"
        ))
        .bind(ws)
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn update_webhook(
        &self,
        ws: Uuid,
        id: Uuid,
        url: Option<&str>,
        events: Option<&str>,
        active: Option<bool>,
        secret: Option<&str>,
    ) -> Result<Webhook> {
        sqlx::query(
            "UPDATE webhooks SET url = COALESCE(?, url), events = COALESCE(?, events), active = COALESCE(?, active), secret = COALESCE(?, secret), updated_at = ?
             WHERE workspace_id = ? AND id = ?",
        )
        .bind(url)
        .bind(events)
        .bind(active)
        .bind(secret)
        .bind(now_ts())
        .bind(ws)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.webhook(ws, id)
            .await?
            .ok_or_else(|| Error::not_found("webhook not found"))
    }

    pub async fn delete_webhook(&self, ws: Uuid, id: Uuid) -> Result<()> {
        let r = sqlx::query("DELETE FROM webhooks WHERE workspace_id = ? AND id = ?")
            .bind(ws)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("webhook not found"));
        }
        Ok(())
    }

    /// Active webhooks of a workspace (by slug) that want `area`.
    pub async fn webhooks_for(&self, slug: &str, area: &str) -> Result<Vec<(Uuid, Uuid)>> {
        let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
            "SELECT h.id, h.workspace_id, h.events FROM webhooks h JOIN workspaces w ON w.id = h.workspace_id
             WHERE w.slug = ? AND w.deleted_at IS NULL AND h.active = 1",
        )
        .bind(slug)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .filter(|(_, _, ev)| ev == "*" || ev.split(',').any(|e| e.trim() == area))
            .map(|(id, ws, _)| (id, ws))
            .collect())
    }

    pub async fn enqueue_delivery(&self, hook: Uuid, event: &str, payload: &str) -> Result<Uuid> {
        let id = new_id();
        let now = now_ts();
        sqlx::query("INSERT INTO webhook_deliveries (id, webhook_id, event, payload, created_at, next_attempt_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(id)
            .bind(hook)
            .bind(event)
            .bind(payload)
            .bind(&now)
            .bind(&now)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(id)
    }

    pub async fn due_deliveries(&self, limit: i64) -> Result<Vec<DueDelivery>> {
        sqlx::query_as(
            "SELECT d.id, h.url, h.secret, d.event, d.payload, d.attempts FROM webhook_deliveries d JOIN webhooks h ON h.id = d.webhook_id
             WHERE d.status = 'pending' AND d.next_attempt_at <= ? AND h.active = 1 ORDER BY d.next_attempt_at LIMIT ?",
        )
        .bind(now_ts())
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    /// Records one attempt; `next` = when to try again (None = no more tries).
    pub async fn record_attempt(
        &self,
        id: Uuid,
        delivered: bool,
        code: Option<i64>,
        error: Option<&str>,
        next: Option<&str>,
    ) -> Result<()> {
        let status = if delivered {
            "delivered"
        } else if next.is_some() {
            "pending"
        } else {
            "failed"
        };
        sqlx::query(
            "UPDATE webhook_deliveries SET status = ?, attempts = attempts + 1, response_code = ?, error = ?, last_attempt_at = ?, next_attempt_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(code)
        .bind(error)
        .bind(now_ts())
        .bind(next)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn deliveries(&self, ws: Uuid, hook: Uuid, limit: i64) -> Result<Vec<Delivery>> {
        sqlx::query_as(
            "SELECT d.id, d.event, d.status, d.attempts, d.response_code, d.error, d.created_at, d.last_attempt_at, d.next_attempt_at, d.payload
             FROM webhook_deliveries d JOIN webhooks h ON h.id = d.webhook_id WHERE h.workspace_id = ? AND d.webhook_id = ? ORDER BY d.created_at DESC LIMIT ?",
        )
        .bind(ws)
        .bind(hook)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    /// Sends a failed or delivered delivery again.
    pub async fn redeliver(&self, ws: Uuid, hook: Uuid, delivery: Uuid) -> Result<()> {
        let r = sqlx::query(
            "UPDATE webhook_deliveries SET status = 'pending', next_attempt_at = ?, attempts = 0
             WHERE id = ? AND webhook_id = ? AND webhook_id IN (SELECT id FROM webhooks WHERE workspace_id = ?)",
        )
        .bind(now_ts())
        .bind(delivery)
        .bind(hook)
        .bind(ws)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("delivery not found"));
        }
        Ok(())
    }

    /// Delivery logs older than `before` are dropped (hourly maintenance).
    pub async fn prune_deliveries(&self, before: &str) -> Result<u64> {
        let r = sqlx::query(
            "DELETE FROM webhook_deliveries WHERE created_at < ? AND status != 'pending'",
        )
        .bind(before)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(r.rows_affected())
    }
}

// ---- GitHub ------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct GithubConn {
    pub workspace_id: Uuid,
    #[serde(skip)]
    pub token: String,
    #[serde(skip)]
    pub secret: String,
    pub bot_id: Uuid,
    pub close_on_merge: bool,
    pub created_at: String,
    pub last_event_at: Option<String>,
}

const GH_COLS: &str =
    "workspace_id, token, secret, bot_id, close_on_merge, created_at, last_event_at";

impl Store {
    pub async fn github(&self, ws: Uuid) -> Result<Option<GithubConn>> {
        sqlx::query_as(&format!(
            "SELECT {GH_COLS} FROM github_connections WHERE workspace_id = ?"
        ))
        .bind(ws)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn github_by_token(&self, token: &str) -> Result<Option<GithubConn>> {
        sqlx::query_as(&format!(
            "SELECT {GH_COLS} FROM github_connections WHERE token = ?"
        ))
        .bind(token)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn connect_github(
        &self,
        ws: Uuid,
        token: &str,
        secret: &str,
        by: Uuid,
    ) -> Result<GithubConn> {
        let bot = self.service_bot(ws, "GitHub").await?;
        sqlx::query(
            "INSERT INTO github_connections (workspace_id, token, secret, bot_id, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (workspace_id) DO UPDATE SET token = excluded.token, secret = excluded.secret",
        )
        .bind(ws)
        .bind(token)
        .bind(secret)
        .bind(bot)
        .bind(by)
        .bind(now_ts())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.github(ws)
            .await?
            .ok_or_else(|| Error::not_found("not connected"))
    }

    pub async fn set_github_close_on_merge(&self, ws: Uuid, on: bool) -> Result<()> {
        sqlx::query("UPDATE github_connections SET close_on_merge = ? WHERE workspace_id = ?")
            .bind(on)
            .bind(ws)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn disconnect_github(&self, ws: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM github_connections WHERE workspace_id = ?")
            .bind(ws)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn github_seen(&self, ws: Uuid) -> Result<()> {
        sqlx::query("UPDATE github_connections SET last_event_at = ? WHERE workspace_id = ?")
            .bind(now_ts())
            .bind(ws)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Adds a link to a work item, or renames the link with the same URL. True if it is new.
    pub async fn upsert_link(&self, item: Uuid, url: &str, title: &str, by: Uuid) -> Result<bool> {
        let r =
            sqlx::query("UPDATE work_item_links SET title = ? WHERE work_item_id = ? AND url = ?")
                .bind(title)
                .bind(item)
                .bind(url)
                .execute(self.pool())
                .await
                .map_err(db_err)?;
        if r.rows_affected() > 0 {
            return Ok(false);
        }
        self.add_link(item, url, title, by).await?;
        Ok(true)
    }
}

impl Store {
    /// The key prefix of a project (`WEB`).
    pub async fn project_identifier(&self, project: Uuid) -> Result<Option<String>> {
        sqlx::query_scalar("SELECT identifier FROM projects WHERE id = ?")
            .bind(project)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)
    }

    /// Identifiers and ids of a workspace's live projects (for finding keys in text).
    pub async fn project_identifiers(&self, ws: Uuid) -> Result<Vec<(String, Uuid)>> {
        sqlx::query_as(
            "SELECT identifier, id FROM projects WHERE workspace_id = ? AND deleted_at IS NULL",
        )
        .bind(ws)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }
}
