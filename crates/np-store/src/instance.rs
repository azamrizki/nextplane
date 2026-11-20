//! The single `instance` row: name, whether setup is done, and the setup token hash.

use np_core::{Error, Result};

use crate::{Store, db_err, now_ts};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignupPolicy {
    /// No new accounts: invite links work only for people who already have one.
    Closed,
    /// Invite links may create accounts (the default).
    Invite,
    /// Anyone who can reach the server may sign up.
    Open,
}

impl SignupPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            SignupPolicy::Closed => "closed",
            SignupPolicy::Invite => "invite",
            SignupPolicy::Open => "open",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "closed" => SignupPolicy::Closed,
            "open" => SignupPolicy::Open,
            _ => SignupPolicy::Invite,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub name: String,
    pub setup_completed: bool,
    pub signup_policy: SignupPolicy,
}

impl Store {
    pub async fn instance(&self) -> Result<Instance> {
        let (name, done, policy): (String, Option<String>, String) = sqlx::query_as(
            "SELECT name, setup_completed_at, signup_policy FROM instance WHERE id = 1",
        )
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(Instance {
            name,
            setup_completed: done.is_some(),
            signup_policy: SignupPolicy::parse(&policy),
        })
    }

    pub async fn update_instance(
        &self,
        name: Option<&str>,
        policy: Option<SignupPolicy>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE instance SET name = COALESCE(?, name), signup_policy = COALESCE(?, signup_policy), updated_at = ? WHERE id = 1",
        )
        .bind(name)
        .bind(policy.map(SignupPolicy::as_str))
        .bind(now_ts())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    /// Stores a new setup token hash, replacing any earlier one. Refused after setup.
    pub async fn set_setup_token(&self, hash: &[u8], expires_at: &str) -> Result<()> {
        let r = sqlx::query(
            "UPDATE instance SET setup_token_hash = ?, setup_token_expires_at = ?, updated_at = ?
             WHERE id = 1 AND setup_completed_at IS NULL",
        )
        .bind(hash)
        .bind(expires_at)
        .bind(now_ts())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::conflict("setup is already complete"));
        }
        Ok(())
    }

    /// The stored setup token hash and expiry, if setup is still open.
    pub async fn setup_token(&self) -> Result<Option<(Vec<u8>, String)>> {
        let row: (Option<Vec<u8>>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT setup_token_hash, setup_token_expires_at, setup_completed_at FROM instance WHERE id = 1",
        )
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(match row {
            (Some(h), Some(exp), None) => Some((h, exp)),
            _ => None,
        })
    }

    /// Finishes setup in one transaction: names the instance, creates the first admin and
    /// clears the token. Fails with `conflict` if setup finished in the meantime.
    pub async fn complete_setup(
        &self,
        instance_name: &str,
        email: &str,
        name: &str,
        password_hash: &str,
    ) -> Result<np_core::User> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let now = now_ts();
        let r = sqlx::query(
            "UPDATE instance SET name = ?, setup_completed_at = ?, setup_token_hash = NULL,
             setup_token_expires_at = NULL, updated_at = ? WHERE id = 1 AND setup_completed_at IS NULL",
        )
        .bind(instance_name)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::conflict("setup is already complete"));
        }
        let user =
            crate::users::insert_user(&mut tx, email, name, Some(password_hash), true).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(user)
    }
}
