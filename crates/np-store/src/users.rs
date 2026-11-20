//! Users.

use np_core::id::{Uuid, new_id};
use np_core::{Error, Result, User};
use sqlx::{FromRow, Sqlite, Transaction};

use crate::{Store, db_err, now_ts};

#[derive(FromRow)]
pub(crate) struct UserRow {
    id: Uuid,
    email: String,
    name: String,
    avatar: Option<String>,
    is_instance_admin: bool,
    is_bot: bool,
    created_at: String,
}

impl From<UserRow> for User {
    fn from(r: UserRow) -> Self {
        User {
            id: r.id,
            email: r.email,
            name: r.name,
            avatar: r.avatar,
            is_instance_admin: r.is_instance_admin,
            is_bot: r.is_bot,
            created_at: r.created_at,
        }
    }
}

pub(crate) const USER_COLS: &str = "id, email, name, avatar, is_instance_admin, is_bot, created_at";

pub(crate) async fn insert_user(
    tx: &mut Transaction<'_, Sqlite>,
    email: &str,
    name: &str,
    password_hash: Option<&str>,
    is_instance_admin: bool,
) -> Result<User> {
    let id = new_id();
    let now = now_ts();
    sqlx::query(
        "INSERT INTO users (id, email, name, password_hash, is_instance_admin, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(email)
    .bind(name)
    .bind(password_hash)
    .bind(is_instance_admin)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .map_err(|e| match db_err(e) {
        e if e.kind == np_core::ErrorKind::Conflict => {
            Error::conflict("an account with this email already exists")
        }
        e => e,
    })?;
    Ok(User {
        id,
        email: email.to_string(),
        name: name.to_string(),
        avatar: None,
        is_instance_admin,
        is_bot: false,
        created_at: now,
    })
}

impl Store {
    pub async fn create_user(
        &self,
        email: &str,
        name: &str,
        password_hash: Option<&str>,
        is_instance_admin: bool,
    ) -> Result<User> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let u = insert_user(&mut tx, email, name, password_hash, is_instance_admin).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(u)
    }

    pub async fn user(&self, id: Uuid) -> Result<Option<User>> {
        let row: Option<UserRow> = sqlx::query_as(&format!(
            "SELECT {USER_COLS} FROM users WHERE id = ? AND deleted_at IS NULL"
        ))
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        Ok(row.map(Into::into))
    }

    /// The user with this (normalized) email and their password hash, for login.
    pub async fn user_for_login(&self, email: &str) -> Result<Option<(User, Option<String>)>> {
        #[derive(FromRow)]
        struct Row {
            #[sqlx(flatten)]
            user: UserRow,
            password_hash: Option<String>,
        }
        let row: Option<Row> = sqlx::query_as(&format!(
            "SELECT {USER_COLS}, password_hash FROM users WHERE email = ? AND deleted_at IS NULL AND is_bot = 0"
        ))
        .bind(email)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        Ok(row.map(|r| (r.user.into(), r.password_hash)))
    }
}

pub(crate) async fn set_password_tx(
    tx: &mut Transaction<'_, Sqlite>,
    user: Uuid,
    password_hash: &str,
) -> Result<()> {
    sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(password_hash)
        .bind(now_ts())
        .bind(user)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    Ok(())
}

impl Store {
    pub async fn user_by_email(&self, email: &str) -> Result<Option<User>> {
        let row: Option<UserRow> = sqlx::query_as(&format!(
            "SELECT {USER_COLS} FROM users WHERE email = ? AND deleted_at IS NULL"
        ))
        .bind(email)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        Ok(row.map(Into::into))
    }

    pub async fn password_hash(&self, user: Uuid) -> Result<Option<String>> {
        let h: Option<Option<String>> =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
                .bind(user)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err)?;
        Ok(h.flatten())
    }

    /// Changes a password and ends every session except `keep_session`.
    pub async fn change_password(
        &self,
        user: Uuid,
        password_hash: &str,
        keep_session: Option<&[u8]>,
    ) -> Result<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        set_password_tx(&mut tx, user, password_hash).await?;
        sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token_hash IS NOT ?")
            .bind(user)
            .bind(keep_session)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }

    pub async fn update_profile(
        &self,
        user: Uuid,
        name: Option<&str>,
        avatar: Option<Option<&str>>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE users SET name = COALESCE(?, name), avatar = CASE WHEN ? THEN ? ELSE avatar END, updated_at = ? WHERE id = ?",
        )
        .bind(name)
        .bind(avatar.is_some())
        .bind(avatar.flatten())
        .bind(now_ts())
        .bind(user)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn all_users(&self) -> Result<Vec<User>> {
        let rows: Vec<UserRow> = sqlx::query_as(&format!(
            "SELECT {USER_COLS} FROM users WHERE deleted_at IS NULL ORDER BY created_at"
        ))
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn count_instance_admins(&self) -> Result<usize> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE is_instance_admin = 1 AND deleted_at IS NULL",
        )
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(n as usize)
    }

    pub async fn set_instance_admin(&self, user: Uuid, on: bool) -> Result<()> {
        sqlx::query("UPDATE users SET is_instance_admin = ?, updated_at = ? WHERE id = ?")
            .bind(on)
            .bind(now_ts())
            .bind(user)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }
}
