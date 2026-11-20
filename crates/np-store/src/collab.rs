//! Comments, reactions, files, links and subscriptions on work items.

use np_core::id::{Uuid, new_id};
use np_core::{Error, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::{Store, db_err, now_ts};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Comment {
    pub id: Uuid,
    pub work_item_id: Uuid,
    pub author_id: Uuid,
    #[sqlx(skip)]
    pub body_json: Option<serde_json::Value>,
    #[serde(skip)]
    pub body_json_raw: Option<String>,
    pub body_html: String,
    pub created_at: String,
    pub edited_at: Option<String>,
    pub hidden_at: Option<String>,
    #[sqlx(skip)]
    pub reactions: Vec<ReactionCount>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReactionCount {
    pub emoji: String,
    pub user_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct FileMeta {
    pub id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub work_item_id: Option<Uuid>,
    pub purpose: String,
    pub uploaded_by: Uuid,
    pub file_name: String,
    pub mime: String,
    pub size: i64,
    #[serde(skip)]
    pub path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Link {
    pub id: Uuid,
    pub url: String,
    pub title: String,
    pub created_by: Uuid,
    pub created_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct NewFile {
    pub workspace_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub work_item_id: Option<Uuid>,
    pub purpose: &'static str,
    pub file_name: String,
    pub mime: String,
    pub size: i64,
    pub path: String,
}

const C_COLS: &str = "id, work_item_id, author_id, body_json AS body_json_raw, body_html, created_at, edited_at, hidden_at";

impl Store {
    // ---- comments -----------------------------------------------------------------------

    pub async fn add_comment(
        &self,
        item: Uuid,
        author: Uuid,
        json: Option<&str>,
        html: &str,
    ) -> Result<Comment> {
        let id = new_id();
        let now = now_ts();
        sqlx::query(
            "INSERT INTO comments (id, work_item_id, author_id, body_json, body_html, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(item)
        .bind(author)
        .bind(json)
        .bind(html)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.index_comment(id).await?;
        self.comment(item, id)
            .await?
            .ok_or_else(|| Error::not_found("comment not found"))
    }

    pub async fn comment(&self, item: Uuid, id: Uuid) -> Result<Option<Comment>> {
        let c: Option<Comment> = sqlx::query_as(&format!(
            "SELECT {C_COLS} FROM comments WHERE id = ? AND work_item_id = ? AND deleted_at IS NULL"
        ))
        .bind(id)
        .bind(item)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        match c {
            Some(c) => Ok(self.with_reactions(vec![c]).await?.pop()),
            None => Ok(None),
        }
    }

    pub async fn comments(&self, item: Uuid) -> Result<Vec<Comment>> {
        let cs: Vec<Comment> = sqlx::query_as(&format!(
            "SELECT {C_COLS} FROM comments WHERE work_item_id = ? AND deleted_at IS NULL ORDER BY created_at"
        ))
        .bind(item)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        self.with_reactions(cs).await
    }

    async fn with_reactions(&self, mut cs: Vec<Comment>) -> Result<Vec<Comment>> {
        for c in &mut cs {
            c.body_json = c
                .body_json_raw
                .take()
                .and_then(|j| serde_json::from_str(&j).ok());
            c.reactions = self.reactions("comment", c.id).await?;
            if c.hidden_at.is_some() {
                c.body_html = String::new();
                c.body_json = None;
            }
        }
        Ok(cs)
    }

    pub async fn edit_comment(&self, id: Uuid, json: Option<&str>, html: &str) -> Result<()> {
        let now = now_ts();
        sqlx::query("UPDATE comments SET body_json = ?, body_html = ?, edited_at = ?, updated_at = ? WHERE id = ?")
            .bind(json)
            .bind(html)
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        self.index_comment(id).await
    }

    pub async fn set_comment_hidden(&self, id: Uuid, by: Uuid, hidden: bool) -> Result<()> {
        sqlx::query("UPDATE comments SET hidden_at = CASE WHEN ? THEN ? ELSE NULL END, hidden_by = CASE WHEN ? THEN ? ELSE NULL END WHERE id = ?")
            .bind(hidden)
            .bind(now_ts())
            .bind(hidden)
            .bind(by)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        self.index_comment(id).await
    }

    pub async fn delete_comment(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE comments SET deleted_at = ? WHERE id = ?")
            .bind(now_ts())
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        self.index_comment(id).await
    }

    // ---- reactions ----------------------------------------------------------------------

    pub async fn reactions(&self, kind: &str, target: Uuid) -> Result<Vec<ReactionCount>> {
        let rows: Vec<(String, Uuid)> = sqlx::query_as(
            "SELECT emoji, user_id FROM reactions WHERE target_kind = ? AND target_id = ? ORDER BY created_at",
        )
        .bind(kind)
        .bind(target)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        let mut out: Vec<ReactionCount> = Vec::new();
        for (emoji, user) in rows {
            match out.iter_mut().find(|r| r.emoji == emoji) {
                Some(r) => r.user_ids.push(user),
                None => out.push(ReactionCount {
                    emoji,
                    user_ids: vec![user],
                }),
            }
        }
        Ok(out)
    }

    /// Adds the reaction, or removes it if the user already gave it.
    pub async fn toggle_reaction(
        &self,
        kind: &str,
        target: Uuid,
        user: Uuid,
        emoji: &str,
    ) -> Result<Vec<ReactionCount>> {
        let r = sqlx::query("DELETE FROM reactions WHERE target_kind = ? AND target_id = ? AND user_id = ? AND emoji = ?")
            .bind(kind)
            .bind(target)
            .bind(user)
            .bind(emoji)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if r.rows_affected() == 0 {
            sqlx::query("INSERT INTO reactions (id, target_kind, target_id, user_id, emoji, created_at) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(new_id())
                .bind(kind)
                .bind(target)
                .bind(user)
                .bind(emoji)
                .bind(now_ts())
                .execute(self.pool())
                .await
                .map_err(db_err)?;
        }
        self.reactions(kind, target).await
    }

    // ---- files --------------------------------------------------------------------------

    pub async fn add_file(&self, f: &NewFile, by: Uuid) -> Result<FileMeta> {
        let id = new_id();
        sqlx::query(
            "INSERT INTO files (id, workspace_id, project_id, work_item_id, purpose, uploaded_by, file_name, mime, size, path, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(f.workspace_id)
        .bind(f.project_id)
        .bind(f.work_item_id)
        .bind(f.purpose)
        .bind(by)
        .bind(&f.file_name)
        .bind(&f.mime)
        .bind(f.size)
        .bind(&f.path)
        .bind(now_ts())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.file(id)
            .await?
            .ok_or_else(|| Error::not_found("file not found"))
    }

    pub async fn file(&self, id: Uuid) -> Result<Option<FileMeta>> {
        sqlx::query_as(
            "SELECT id, workspace_id, project_id, work_item_id, purpose, uploaded_by, file_name, mime, size, path, created_at
             FROM files WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn work_item_files(&self, item: Uuid) -> Result<Vec<FileMeta>> {
        sqlx::query_as(
            "SELECT id, workspace_id, project_id, work_item_id, purpose, uploaded_by, file_name, mime, size, path, created_at
             FROM files WHERE work_item_id = ? AND purpose = 'attachment' AND deleted_at IS NULL ORDER BY created_at",
        )
        .bind(item)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    /// Marks a file deleted. The bytes stay on disk (backups and restores stay consistent);
    /// nothing serves a deleted file.
    pub async fn delete_file(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE files SET deleted_at = ? WHERE id = ?")
            .bind(now_ts())
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    // ---- links --------------------------------------------------------------------------

    pub async fn links_of(&self, item: Uuid) -> Result<Vec<Link>> {
        sqlx::query_as("SELECT id, url, title, created_by, created_at FROM work_item_links WHERE work_item_id = ? ORDER BY created_at")
            .bind(item)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }

    pub async fn add_link(&self, item: Uuid, url: &str, title: &str, by: Uuid) -> Result<()> {
        sqlx::query("INSERT INTO work_item_links (id, work_item_id, url, title, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(new_id())
            .bind(item)
            .bind(url)
            .bind(title)
            .bind(by)
            .bind(now_ts())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn link(&self, item: Uuid, id: Uuid) -> Result<Option<Link>> {
        sqlx::query_as("SELECT id, url, title, created_by, created_at FROM work_item_links WHERE id = ? AND work_item_id = ?")
            .bind(id)
            .bind(item)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)
    }

    pub async fn remove_link(&self, item: Uuid, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM work_item_links WHERE id = ? AND work_item_id = ?")
            .bind(id)
            .bind(item)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    // ---- subscriptions ------------------------------------------------------------------

    pub async fn subscribe(&self, item: Uuid, user: Uuid) -> Result<()> {
        sqlx::query("INSERT OR IGNORE INTO subscriptions (work_item_id, user_id, created_at) VALUES (?, ?, ?)")
            .bind(item)
            .bind(user)
            .bind(now_ts())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn unsubscribe(&self, item: Uuid, user: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM subscriptions WHERE work_item_id = ? AND user_id = ?")
            .bind(item)
            .bind(user)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn is_subscribed(&self, item: Uuid, user: Uuid) -> Result<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM subscriptions WHERE work_item_id = ? AND user_id = ?",
        )
        .bind(item)
        .bind(user)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(n > 0)
    }

    pub async fn subscribers(&self, item: Uuid) -> Result<Vec<Uuid>> {
        sqlx::query_scalar("SELECT user_id FROM subscriptions WHERE work_item_id = ?")
            .bind(item)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }
}
