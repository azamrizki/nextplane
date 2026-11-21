//! The home page's widgets (S3.7): widget preferences, quick links, recent visits and stickies.
//! Everything here belongs to one person in one workspace.

use np_core::id::{Uuid, new_id};
use np_core::{Error, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::{Store, db_err, now_ts};

/// The widgets in their default order (shown highest `sort_order` first).
pub const WIDGETS: [&str; 3] = ["quick_links", "recents", "stickies"];
/// Recent visits kept per person and workspace.
pub const RECENTS_KEPT: i64 = 20;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Widget {
    pub key: String,
    pub is_enabled: bool,
    pub sort_order: f64,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct QuickLink {
    pub id: Uuid,
    pub url: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Visit {
    pub entity: String,
    pub entity_id: Uuid,
    pub project_id: Option<Uuid>,
    pub visited_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Sticky {
    pub id: Uuid,
    #[sqlx(json(nullable))]
    pub description_json: Option<serde_json::Value>,
    pub description_html: String,
    pub color: String,
    pub sort_order: f64,
    pub created_at: String,
    pub updated_at: String,
}

const STICKY_COLS: &str =
    "id, description_json, description_html, color, sort_order, created_at, updated_at";

#[derive(Debug, Default, Clone)]
pub struct StickyPatch {
    /// (JSON, sanitized HTML, plain text)
    pub body: Option<(Option<String>, String, String)>,
    pub color: Option<String>,
    pub sort_order: Option<f64>,
}

impl Store {
    /// The person's widgets, creating the missing ones in the default order.
    pub async fn home_widgets(&self, ws: Uuid, user: Uuid) -> Result<Vec<Widget>> {
        for (i, key) in WIDGETS.iter().enumerate() {
            sqlx::query(
                "INSERT OR IGNORE INTO home_widgets (user_id, workspace_id, key, is_enabled, sort_order) VALUES (?, ?, ?, 1, ?)",
            )
            .bind(user)
            .bind(ws)
            .bind(key)
            .bind(((WIDGETS.len() - i) * 10_000) as f64)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        }
        sqlx::query_as(
            "SELECT key, is_enabled, sort_order FROM home_widgets WHERE user_id = ? AND workspace_id = ?
             ORDER BY sort_order DESC",
        )
        .bind(user)
        .bind(ws)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn set_home_widget(
        &self,
        ws: Uuid,
        user: Uuid,
        key: &str,
        enabled: Option<bool>,
        sort_order: Option<f64>,
    ) -> Result<()> {
        self.home_widgets(ws, user).await?;
        sqlx::query(
            "UPDATE home_widgets SET is_enabled = COALESCE(?, is_enabled), sort_order = COALESCE(?, sort_order)
             WHERE user_id = ? AND workspace_id = ? AND key = ?",
        )
        .bind(enabled)
        .bind(sort_order)
        .bind(user)
        .bind(ws)
        .bind(key)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn quick_links(&self, ws: Uuid, user: Uuid) -> Result<Vec<QuickLink>> {
        sqlx::query_as(
            "SELECT id, url, title, created_at, updated_at FROM quick_links WHERE workspace_id = ? AND user_id = ?
             ORDER BY created_at DESC, id DESC",
        )
        .bind(ws)
        .bind(user)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    async fn quick_link(&self, ws: Uuid, user: Uuid, id: Uuid) -> Result<QuickLink> {
        sqlx::query_as(
            "SELECT id, url, title, created_at, updated_at FROM quick_links WHERE id = ? AND workspace_id = ? AND user_id = ?",
        )
        .bind(id)
        .bind(ws)
        .bind(user)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?
        .ok_or_else(|| Error::not_found("quick link not found"))
    }

    fn link_taken(e: sqlx::Error) -> Error {
        match db_err(e) {
            e if e.kind == np_core::error::ErrorKind::Conflict => {
                Error::conflict("you already have a quick link to this address")
            }
            e => e,
        }
    }

    pub async fn add_quick_link(
        &self,
        ws: Uuid,
        user: Uuid,
        url: &str,
        title: &str,
    ) -> Result<QuickLink> {
        let id = new_id();
        let now = now_ts();
        sqlx::query(
            "INSERT INTO quick_links (id, workspace_id, user_id, url, title, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(ws)
        .bind(user)
        .bind(url)
        .bind(title)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(Self::link_taken)?;
        self.quick_link(ws, user, id).await
    }

    pub async fn update_quick_link(
        &self,
        ws: Uuid,
        user: Uuid,
        id: Uuid,
        url: Option<&str>,
        title: Option<&str>,
    ) -> Result<QuickLink> {
        self.quick_link(ws, user, id).await?;
        sqlx::query(
            "UPDATE quick_links SET url = COALESCE(?, url), title = COALESCE(?, title), updated_at = ? WHERE id = ?",
        )
        .bind(url)
        .bind(title)
        .bind(now_ts())
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(Self::link_taken)?;
        self.quick_link(ws, user, id).await
    }

    pub async fn delete_quick_link(&self, ws: Uuid, user: Uuid, id: Uuid) -> Result<()> {
        self.quick_link(ws, user, id).await?;
        sqlx::query("DELETE FROM quick_links WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Records (or refreshes) a visit and keeps only the newest [`RECENTS_KEPT`].
    pub async fn record_visit(
        &self,
        ws: Uuid,
        user: Uuid,
        project: Option<Uuid>,
        entity: &str,
        entity_id: Uuid,
    ) -> Result<()> {
        let now = now_ts();
        sqlx::query(
            "INSERT INTO recent_visits (id, workspace_id, user_id, project_id, entity, entity_id, visited_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (workspace_id, user_id, entity, entity_id) DO UPDATE SET visited_at = excluded.visited_at",
        )
        .bind(new_id())
        .bind(ws)
        .bind(user)
        .bind(project)
        .bind(entity)
        .bind(entity_id)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        sqlx::query(
            "DELETE FROM recent_visits WHERE workspace_id = ? AND user_id = ? AND id NOT IN (
               SELECT id FROM recent_visits WHERE workspace_id = ? AND user_id = ? ORDER BY visited_at DESC, id DESC LIMIT ?)",
        )
        .bind(ws)
        .bind(user)
        .bind(ws)
        .bind(user)
        .bind(RECENTS_KEPT)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    /// Recent visits, most recent first.
    pub async fn recent_visits(&self, ws: Uuid, user: Uuid) -> Result<Vec<Visit>> {
        sqlx::query_as(
            "SELECT entity, entity_id, project_id, visited_at FROM recent_visits WHERE workspace_id = ? AND user_id = ?
             ORDER BY visited_at DESC, id DESC",
        )
        .bind(ws)
        .bind(user)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    /// Stickies, highest `sort_order` first; `query` matches the note's text.
    pub async fn stickies(
        &self,
        ws: Uuid,
        user: Uuid,
        query: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Sticky>> {
        let like = query.map(str::trim).filter(|q| !q.is_empty()).map(|q| {
            format!(
                "%{}%",
                q.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        });
        sqlx::query_as(&format!(
            "SELECT {STICKY_COLS} FROM stickies WHERE workspace_id = ? AND user_id = ?
               AND (? IS NULL OR text LIKE ? ESCAPE '\\')
             ORDER BY sort_order DESC, created_at DESC LIMIT ? OFFSET ?"
        ))
        .bind(ws)
        .bind(user)
        .bind(&like)
        .bind(&like)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn sticky(&self, ws: Uuid, user: Uuid, id: Uuid) -> Result<Sticky> {
        sqlx::query_as(&format!(
            "SELECT {STICKY_COLS} FROM stickies WHERE id = ? AND workspace_id = ? AND user_id = ?"
        ))
        .bind(id)
        .bind(ws)
        .bind(user)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?
        .ok_or_else(|| Error::not_found("sticky not found"))
    }

    /// A new sticky on top of the person's others.
    pub async fn create_sticky(
        &self,
        ws: Uuid,
        user: Uuid,
        body: (Option<String>, String, String),
        color: &str,
    ) -> Result<Sticky> {
        let top: Option<f64> = sqlx::query_scalar(
            "SELECT MAX(sort_order) FROM stickies WHERE workspace_id = ? AND user_id = ?",
        )
        .bind(ws)
        .bind(user)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        let id = new_id();
        let now = now_ts();
        sqlx::query(
            "INSERT INTO stickies (id, workspace_id, user_id, description_json, description_html, text, color, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(ws)
        .bind(user)
        .bind(&body.0)
        .bind(&body.1)
        .bind(&body.2)
        .bind(color)
        .bind(top.unwrap_or(0.0) + 10_000.0)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.sticky(ws, user, id).await
    }

    pub async fn update_sticky(
        &self,
        ws: Uuid,
        user: Uuid,
        id: Uuid,
        p: &StickyPatch,
    ) -> Result<Sticky> {
        self.sticky(ws, user, id).await?;
        let (set_body, json, html, text) = match &p.body {
            Some((j, h, t)) => (true, j.clone(), h.clone(), t.clone()),
            None => (false, None, String::new(), String::new()),
        };
        sqlx::query(
            "UPDATE stickies SET
               description_json = CASE WHEN ? THEN ? ELSE description_json END,
               description_html = CASE WHEN ? THEN ? ELSE description_html END,
               text = CASE WHEN ? THEN ? ELSE text END,
               color = COALESCE(?, color), sort_order = COALESCE(?, sort_order), updated_at = ?
             WHERE id = ?",
        )
        .bind(set_body)
        .bind(json)
        .bind(set_body)
        .bind(html)
        .bind(set_body)
        .bind(text)
        .bind(&p.color)
        .bind(p.sort_order)
        .bind(now_ts())
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.sticky(ws, user, id).await
    }

    pub async fn delete_sticky(&self, ws: Uuid, user: Uuid, id: Uuid) -> Result<()> {
        self.sticky(ws, user, id).await?;
        sqlx::query("DELETE FROM stickies WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }
}
