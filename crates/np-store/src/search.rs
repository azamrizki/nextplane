//! Full-text search over work items and comments (SQLite FTS5), and API tokens.

use np_core::id::{Uuid, new_id};
use np_core::{Error, Result, User};
use serde::Serialize;
use sqlx::FromRow;

use crate::users::{USER_COLS, UserRow};
use crate::{Store, db_err, now_ts};

/// Plain text of HTML: tags dropped, common entities decoded, whitespace collapsed.
pub fn html_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Turns user input into an FTS5 query: every word must match, as a prefix.
pub fn fts_query(q: &str) -> Option<String> {
    let terms: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .take(8)
        .map(|t| format!("\"{}\"*", t.replace('"', "")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SearchHit {
    pub kind: String,
    pub work_item_id: Uuid,
    pub project_id: Uuid,
    pub identifier: String,
    pub sequence: i64,
    pub title: String,
    pub snippet: String,
    pub state_id: Uuid,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ApiToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub scope: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

impl Store {
    pub(crate) async fn index_work_item(&self, id: Uuid) -> Result<()> {
        let row: Option<(Uuid, String, String)> =
            sqlx::query_as("SELECT project_id, title, description_html FROM work_items WHERE id = ? AND deleted_at IS NULL AND is_draft = 0")
                .bind(id)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err)?;
        sqlx::query("DELETE FROM search_fts WHERE kind = 'work_item' AND ref_id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if let Some((pid, title, html)) = row {
            sqlx::query("INSERT INTO search_fts (kind, ref_id, project_id, work_item_id, title, body) VALUES ('work_item', ?, ?, ?, ?, ?)")
                .bind(id)
                .bind(pid)
                .bind(id)
                .bind(title)
                .bind(html_text(&html))
                .execute(self.pool())
                .await
                .map_err(db_err)?;
        }
        Ok(())
    }

    pub(crate) async fn index_comment(&self, id: Uuid) -> Result<()> {
        let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
            "SELECT w.project_id, c.work_item_id, c.body_html FROM comments c JOIN work_items w ON w.id = c.work_item_id
             WHERE c.id = ? AND c.deleted_at IS NULL AND c.hidden_at IS NULL",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        sqlx::query("DELETE FROM search_fts WHERE kind = 'comment' AND ref_id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if let Some((pid, wid, html)) = row {
            sqlx::query("INSERT INTO search_fts (kind, ref_id, project_id, work_item_id, title, body) VALUES ('comment', ?, ?, ?, '', ?)")
                .bind(id)
                .bind(pid)
                .bind(wid)
                .bind(html_text(&html))
                .execute(self.pool())
                .await
                .map_err(db_err)?;
        }
        Ok(())
    }

    /// Search in the given projects (the caller passes only projects the user may read).
    pub async fn search(&self, projects: &[Uuid], q: &str, limit: i64) -> Result<Vec<SearchHit>> {
        let Some(m) = fts_query(q) else {
            return Ok(vec![]);
        };
        if projects.is_empty() {
            return Ok(vec![]);
        }
        let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT f.kind, w.id AS work_item_id, w.project_id, p.identifier, w.sequence, w.title,
                    snippet(search_fts, 5, '[', ']', '…', 12) AS snippet, w.state_id
             FROM search_fts f
             JOIN work_items w ON w.id = f.work_item_id AND w.deleted_at IS NULL AND w.is_draft = 0
             JOIN projects p ON p.id = w.project_id AND p.deleted_at IS NULL
             WHERE search_fts MATCH ",
        );
        qb.push_bind(m);
        qb.push(" AND f.project_id IN (");
        let mut sep = qb.separated(", ");
        for p in projects {
            sep.push_bind(*p);
        }
        qb.push(") ORDER BY bm25(search_fts, 0, 0, 0, 0, 5.0, 1.0) LIMIT ");
        qb.push_bind(limit * 3);
        let mut hits: Vec<SearchHit> = qb
            .build_query_as()
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        // One hit per work item, the best one.
        let mut seen = std::collections::HashSet::new();
        hits.retain(|h| seen.insert(h.work_item_id));
        hits.truncate(limit as usize);
        Ok(hits)
    }

    // ---- API tokens ---------------------------------------------------------------------

    pub async fn create_token(
        &self,
        user: Uuid,
        name: &str,
        hash: &[u8],
        scope: &str,
        expires_at: Option<&str>,
        by: Uuid,
    ) -> Result<ApiToken> {
        let id = new_id();
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scope, created_by, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(user)
        .bind(name)
        .bind(hash)
        .bind(scope)
        .bind(by)
        .bind(expires_at)
        .bind(now_ts())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.tokens(user)
            .await?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| Error::not_found("token not found"))
    }

    pub async fn tokens(&self, user: Uuid) -> Result<Vec<ApiToken>> {
        sqlx::query_as(
            "SELECT id, user_id, name, scope, expires_at, last_used_at, created_at FROM api_tokens
             WHERE user_id = ? AND revoked_at IS NULL ORDER BY created_at DESC",
        )
        .bind(user)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn revoke_token(&self, user: Uuid, id: Uuid) -> Result<()> {
        let r = sqlx::query("UPDATE api_tokens SET revoked_at = ? WHERE id = ? AND user_id = ? AND revoked_at IS NULL")
            .bind(now_ts())
            .bind(id)
            .bind(user)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("token not found"));
        }
        Ok(())
    }

    /// The user and scope of a live token; records its use at most once a minute.
    pub async fn token_user(&self, hash: &[u8]) -> Result<Option<(User, String)>> {
        #[derive(FromRow)]
        struct Row {
            tid: Uuid,
            scope: String,
            last_used_at: Option<String>,
            #[sqlx(flatten)]
            user: UserRow,
        }
        let cols = USER_COLS
            .split(", ")
            .map(|c| format!("u.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let now = now_ts();
        let row: Option<Row> = sqlx::query_as(&format!(
            "SELECT t.id AS tid, t.scope, t.last_used_at, {cols} FROM api_tokens t JOIN users u ON u.id = t.user_id
             WHERE t.token_hash = ? AND t.revoked_at IS NULL AND (t.expires_at IS NULL OR t.expires_at > ?) AND u.deleted_at IS NULL"
        ))
        .bind(hash)
        .bind(&now)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        let Some(r) = row else { return Ok(None) };
        if r.last_used_at
            .as_deref()
            .is_none_or(|t| t[..16] != now[..16])
        {
            sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE id = ?")
                .bind(&now)
                .bind(r.tid)
                .execute(self.pool())
                .await
                .map_err(db_err)?;
        }
        Ok(Some((r.user.into(), r.scope)))
    }

    // ---- bots ---------------------------------------------------------------------------

    /// Creates a bot account that is a member of the workspace.
    pub async fn create_bot(
        &self,
        ws: Uuid,
        name: &str,
        role: np_core::perm::WsRole,
    ) -> Result<User> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let id = new_id();
        let now = now_ts();
        let email = format!("bot-{}@bots.nextplane.invalid", id.simple());
        sqlx::query(
            "INSERT INTO users (id, email, name, is_bot, bot_workspace_id, created_at, updated_at) VALUES (?, ?, ?, 1, ?, ?, ?)",
        )
        .bind(id)
        .bind(&email)
        .bind(name)
        .bind(ws)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        crate::workspaces::upsert_member(&mut tx, ws, id, role).await?;
        tx.commit().await.map_err(db_err)?;
        self.user(id)
            .await?
            .ok_or_else(|| Error::not_found("bot not found"))
    }

    pub async fn workspace_bots(&self, ws: Uuid) -> Result<Vec<User>> {
        let rows: Vec<UserRow> = sqlx::query_as(&format!(
            "SELECT {USER_COLS} FROM users u WHERE bot_workspace_id = ? AND is_bot = 1 AND deleted_at IS NULL
               AND EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = ? AND m.user_id = u.id AND m.deactivated_at IS NULL)
             ORDER BY created_at"
        ))
        .bind(ws)
        .bind(ws)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn bot_workspace(&self, user: Uuid) -> Result<Option<Uuid>> {
        let w: Option<Option<Uuid>> =
            sqlx::query_scalar("SELECT bot_workspace_id FROM users WHERE id = ? AND is_bot = 1")
                .bind(user)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err)?;
        Ok(w.flatten())
    }

    /// Removes a bot: its membership ends and all its tokens are revoked.
    pub async fn remove_bot(&self, ws: Uuid, bot: Uuid) -> Result<()> {
        if self.bot_workspace(bot).await? != Some(ws) {
            return Err(Error::not_found("bot not found"));
        }
        self.deactivate_ws_member(ws, bot).await?;
        sqlx::query(
            "UPDATE api_tokens SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(now_ts())
        .bind(bot)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_text() {
        assert_eq!(fts_query("login bug!").unwrap(), "\"login\"* \"bug\"*");
        assert_eq!(
            fts_query("\" OR 1=1 --"),
            Some("\"OR\"* \"1\"* \"1\"*".into())
        );
        assert!(fts_query("  -- ").is_none());
        assert_eq!(html_text("<p>Hi &amp; <b>bye</b></p>"), "Hi & bye");
    }
}
