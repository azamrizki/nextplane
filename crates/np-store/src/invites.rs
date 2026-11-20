//! Workspace invites and password-reset links. Only token hashes are stored.

use np_core::id::{Uuid, new_id};
use np_core::perm::WsRole;
use np_core::{Error, Result, User};
use serde::Serialize;

use crate::workspaces::upsert_member;
use crate::{Store, db_err, now_ts};

#[derive(Debug, Clone, Serialize)]
pub struct Invite {
    pub id: Uuid,
    pub email: String,
    pub role: WsRole,
    pub invited_by: Uuid,
    pub expires_at: String,
    pub created_at: String,
}

/// What an invite link shows before it is accepted.
#[derive(Debug, Clone, Serialize)]
pub struct InviteInfo {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub workspace_slug: String,
    pub workspace_name: String,
    pub email: String,
    pub role: WsRole,
    pub invited_by_name: String,
    pub account_exists: bool,
}

impl Store {
    /// Creates an invite; an earlier pending invite for the same email is revoked first.
    pub async fn create_invite(
        &self,
        ws: Uuid,
        email: &str,
        role: WsRole,
        token_hash: &[u8],
        invited_by: Uuid,
        expires_at: &str,
    ) -> Result<Invite> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let now = now_ts();
        sqlx::query(
            "UPDATE invites SET revoked_at = ? WHERE workspace_id = ? AND email = ? AND accepted_at IS NULL AND revoked_at IS NULL",
        )
        .bind(&now)
        .bind(ws)
        .bind(email)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        let id = new_id();
        sqlx::query(
            "INSERT INTO invites (id, workspace_id, email, role, token_hash, invited_by, expires_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(ws)
        .bind(email)
        .bind(role.as_str())
        .bind(token_hash)
        .bind(invited_by)
        .bind(expires_at)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(Invite {
            id,
            email: email.into(),
            role,
            invited_by,
            expires_at: expires_at.into(),
            created_at: now,
        })
    }

    pub async fn pending_invites(&self, ws: Uuid) -> Result<Vec<Invite>> {
        let rows: Vec<(Uuid, String, String, Uuid, String, String)> = sqlx::query_as(
            "SELECT id, email, role, invited_by, expires_at, created_at FROM invites
             WHERE workspace_id = ? AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > ?
             ORDER BY created_at DESC",
        )
        .bind(ws)
        .bind(now_ts())
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        rows.into_iter()
            .map(|(id, email, role, invited_by, expires_at, created_at)| {
                Ok(Invite {
                    id,
                    email,
                    role: WsRole::parse(&role)?,
                    invited_by,
                    expires_at,
                    created_at,
                })
            })
            .collect()
    }

    pub async fn revoke_invite(&self, ws: Uuid, id: Uuid) -> Result<()> {
        let r = sqlx::query(
            "UPDATE invites SET revoked_at = ? WHERE id = ? AND workspace_id = ? AND accepted_at IS NULL AND revoked_at IS NULL",
        )
        .bind(now_ts())
        .bind(id)
        .bind(ws)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("invite not found"));
        }
        Ok(())
    }

    /// A pending, unexpired invite in a live workspace, by token hash.
    pub async fn invite_by_token(&self, token_hash: &[u8]) -> Result<Option<InviteInfo>> {
        let row: Option<InviteRow> = sqlx::query_as(
            "SELECT i.id, w.id, w.slug, w.name, i.email, i.role,
                    COALESCE(NULLIF(u.name, ''), u.email),
                    i.expires_at,
                    EXISTS (SELECT 1 FROM users x WHERE x.email = i.email AND x.deleted_at IS NULL)
             FROM invites i JOIN workspaces w ON w.id = i.workspace_id JOIN users u ON u.id = i.invited_by
             WHERE i.token_hash = ? AND i.accepted_at IS NULL AND i.revoked_at IS NULL
               AND i.expires_at > ? AND w.deleted_at IS NULL",
        )
        .bind(token_hash)
        .bind(now_ts())
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        row.map(
            |(
                id,
                workspace_id,
                workspace_slug,
                workspace_name,
                email,
                role,
                invited_by_name,
                _,
                exists,
            )| {
                Ok(InviteInfo {
                    id,
                    workspace_id,
                    workspace_slug,
                    workspace_name,
                    email,
                    role: WsRole::parse(&role)?,
                    invited_by_name,
                    account_exists: exists != 0,
                })
            },
        )
        .transpose()
    }

    /// Accepts an invite for an existing user: marks it used and adds (or reactivates) the
    /// membership. An active member keeps their current role.
    pub async fn accept_invite(&self, invite: &InviteInfo, user: Uuid) -> Result<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        mark_accepted(&mut tx, invite.id).await?;
        upsert_member(&mut tx, invite.workspace_id, user, invite.role).await?;
        tx.commit().await.map_err(db_err)
    }

    /// Accepts an invite by creating the account it was sent to.
    pub async fn accept_invite_with_signup(
        &self,
        invite: &InviteInfo,
        name: &str,
        password_hash: &str,
    ) -> Result<User> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        mark_accepted(&mut tx, invite.id).await?;
        let user =
            crate::users::insert_user(&mut tx, &invite.email, name, Some(password_hash), false)
                .await?;
        upsert_member(&mut tx, invite.workspace_id, user.id, invite.role).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(user)
    }

    pub async fn create_password_reset(
        &self,
        user: Uuid,
        token_hash: &[u8],
        expires_at: &str,
        created_by: Option<Uuid>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO password_resets (id, user_id, token_hash, created_by, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(user)
        .bind(token_hash)
        .bind(created_by)
        .bind(expires_at)
        .bind(now_ts())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    /// The user a live reset token belongs to.
    pub async fn password_reset_user(&self, token_hash: &[u8]) -> Result<Option<User>> {
        let id: Option<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM password_resets WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now_ts())
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        match id {
            Some(id) => self.user(id).await,
            None => Ok(None),
        }
    }

    /// Uses a reset token: sets the password, marks every open reset for the user used and
    /// ends all their sessions. Fails with `not_found` if the token is no longer valid.
    pub async fn use_password_reset(&self, token_hash: &[u8], password_hash: &str) -> Result<User> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let now = now_ts();
        let user: Option<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM password_resets WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(&now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        let user =
            user.ok_or_else(|| Error::not_found("this reset link is invalid or has expired"))?;
        sqlx::query("UPDATE password_resets SET used_at = ? WHERE user_id = ? AND used_at IS NULL")
            .bind(&now)
            .bind(user)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        crate::users::set_password_tx(&mut tx, user, password_hash).await?;
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(user)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        self.user(user)
            .await?
            .ok_or_else(|| Error::not_found("user not found"))
    }
}

async fn mark_accepted(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, id: Uuid) -> Result<()> {
    let r = sqlx::query("UPDATE invites SET accepted_at = ? WHERE id = ? AND accepted_at IS NULL AND revoked_at IS NULL")
        .bind(now_ts())
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    if r.rows_affected() == 0 {
        return Err(Error::conflict("this invite was already used or revoked"));
    }
    Ok(())
}

impl Store {
    /// Whether someone invited this email to a live workspace (and the invite is still good).
    pub async fn has_pending_invite(&self, email: &str) -> Result<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invites i JOIN workspaces w ON w.id = i.workspace_id
             WHERE i.email = ? AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at > ? AND w.deleted_at IS NULL",
        )
        .bind(email)
        .bind(now_ts())
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(n > 0)
    }
}

/// (invite, workspace, slug, name, email, role, inviter, expires, account exists)
type InviteRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
);
