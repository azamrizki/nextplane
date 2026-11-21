//! Comments, reactions and votes on public boards (S3.7, as in Plane's published boards).

use np_core::Result;
use np_core::id::Uuid;
use serde::Serialize;
use sqlx::FromRow;

use crate::{Store, db_err, now_ts};

/// A published board and what visitors may do on it.
#[derive(Debug, Clone, FromRow)]
pub struct BoardInfo {
    pub workspace_name: String,
    pub workspace_id: Uuid,
    pub workspace_slug: String,
    pub project_id: Uuid,
    pub name: String,
    pub identifier: String,
    pub comments: bool,
    pub reactions: bool,
    pub votes: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PublicComment {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_name: String,
    pub author_avatar: Option<String>,
    pub body_html: String,
    pub created_at: String,
    pub edited_at: Option<String>,
}

impl Store {
    pub async fn board_info(&self, token: &str) -> Result<Option<BoardInfo>> {
        sqlx::query_as(
            "SELECT w.name AS workspace_name, w.id AS workspace_id, w.slug AS workspace_slug, p.id AS project_id, p.name,
               p.identifier, p.board_comments AS comments, p.board_reactions AS reactions, p.board_votes AS votes
             FROM projects p JOIN workspaces w ON w.id = p.workspace_id
             WHERE p.public_board_token = ? AND p.deleted_at IS NULL AND w.deleted_at IS NULL",
        )
        .bind(token)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    /// (comments, reactions, votes) allowed on a project's board.
    pub async fn board_settings(&self, project: Uuid) -> Result<(bool, bool, bool)> {
        sqlx::query_as("SELECT board_comments, board_reactions, board_votes FROM projects WHERE id = ?")
            .bind(project)
            .fetch_one(self.pool())
            .await
            .map_err(db_err)
    }

    pub async fn set_board_settings(
        &self,
        project: Uuid,
        comments: Option<bool>,
        reactions: Option<bool>,
        votes: Option<bool>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE projects SET board_comments = COALESCE(?, board_comments), board_reactions = COALESCE(?, board_reactions),
               board_votes = COALESCE(?, board_votes) WHERE id = ?",
        )
        .bind(comments)
        .bind(reactions)
        .bind(votes)
        .bind(project)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    /// The id of a work item the board shows (live, published, not archived).
    pub async fn board_item_id(&self, project: Uuid, sequence: i64) -> Result<Option<Uuid>> {
        sqlx::query_scalar(
            "SELECT id FROM work_items WHERE project_id = ? AND sequence = ? AND deleted_at IS NULL AND is_draft = 0 AND archived_at IS NULL",
        )
        .bind(project)
        .bind(sequence)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    /// Public comments of an item, newest first (as on Plane's boards); hidden ones left out.
    pub async fn public_comments(&self, item: Uuid) -> Result<Vec<PublicComment>> {
        sqlx::query_as(
            "SELECT c.id, c.author_id, COALESCE(NULLIF(u.name, ''), 'Someone') AS author_name, u.avatar AS author_avatar,
               c.body_html, c.created_at, c.edited_at
             FROM comments c JOIN users u ON u.id = c.author_id
             WHERE c.work_item_id = ? AND c.is_public = 1 AND c.deleted_at IS NULL AND c.hidden_at IS NULL
             ORDER BY c.created_at DESC",
        )
        .bind(item)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn set_comment_public(&self, comment: Uuid, public: bool) -> Result<()> {
        sqlx::query("UPDATE comments SET is_public = ? WHERE id = ?")
            .bind(public)
            .bind(comment)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Everyone's votes on an item: (person, +1 or -1).
    pub async fn votes(&self, item: Uuid) -> Result<Vec<(Uuid, i64)>> {
        sqlx::query_as("SELECT user_id, value FROM work_item_votes WHERE work_item_id = ? ORDER BY created_at")
            .bind(item)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }

    /// One vote per person: +1, -1, or 0 to take it back.
    pub async fn set_vote(&self, item: Uuid, user: Uuid, value: i64) -> Result<()> {
        if value == 0 {
            sqlx::query("DELETE FROM work_item_votes WHERE work_item_id = ? AND user_id = ?")
                .bind(item)
                .bind(user)
                .execute(self.pool())
                .await
                .map_err(db_err)?;
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO work_item_votes (work_item_id, user_id, value, created_at) VALUES (?, ?, ?, ?)
             ON CONFLICT (work_item_id, user_id) DO UPDATE SET value = excluded.value",
        )
        .bind(item)
        .bind(user)
        .bind(value)
        .bind(now_ts())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }
}
