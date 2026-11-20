//! Login sessions. The cookie holds a random token; only its SHA-256 hash is stored.

use np_core::id::{Uuid, new_id};
use np_core::{Result, User};
use sqlx::FromRow;

use crate::users::{USER_COLS, UserRow};
use crate::{Store, db_err, now_ts};

impl Store {
    pub async fn create_session(
        &self,
        user_id: Uuid,
        token_hash: &[u8],
        expires_at: &str,
        user_agent: &str,
        ip: &str,
    ) -> Result<Uuid> {
        let id = new_id();
        let now = now_ts();
        sqlx::query(
            "INSERT INTO sessions (id, user_id, token_hash, expires_at, last_seen_at, user_agent, ip, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .bind(&now)
        .bind(user_agent)
        .bind(ip)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(id)
    }

    /// The live session for a token hash and its user. Expired sessions and deleted users
    /// count as no session.
    pub async fn session_user(&self, token_hash: &[u8]) -> Result<Option<(Uuid, String, User)>> {
        #[derive(FromRow)]
        struct Row {
            session_id: Uuid,
            last_seen_at: String,
            #[sqlx(flatten)]
            user: UserRow,
        }
        let cols = USER_COLS
            .split(", ")
            .map(|c| format!("u.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let row: Option<Row> = sqlx::query_as(&format!(
            "SELECT s.id AS session_id, s.last_seen_at, {cols}
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ? AND s.expires_at > ? AND u.deleted_at IS NULL"
        ))
        .bind(token_hash)
        .bind(now_ts())
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        Ok(row.map(|r| (r.session_id, r.last_seen_at, r.user.into())))
    }

    /// Slides the session: new expiry and last-seen time.
    pub async fn touch_session(&self, id: Uuid, expires_at: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE id = ?")
            .bind(now_ts())
            .bind(expires_at)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn delete_session_by_hash(&self, token_hash: &[u8]) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Removes expired sessions; returns how many.
    pub async fn purge_expired_sessions(&self) -> Result<u64> {
        let r = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(now_ts())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(r.rows_affected())
    }
}
