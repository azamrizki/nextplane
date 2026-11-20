//! Workspaces and their members.

use np_core::id::{Uuid, new_id};
use np_core::perm::WsRole;
use np_core::{Error, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::{Store, db_err, now_ts};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Workspace {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub logo: Option<String>,
    pub members_can_create_projects: bool,
    pub created_at: String,
    pub deleted_at: Option<String>,
}

/// A workspace together with the caller's role in it.
#[derive(Debug, Clone, Serialize)]
pub struct MyWorkspace {
    #[serde(flatten)]
    pub workspace: Workspace,
    pub role: WsRole,
}

#[derive(Debug, Clone, Serialize)]
pub struct WsMember {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub avatar: Option<String>,
    pub role: WsRole,
    pub joined_at: String,
}

const WS_COLS: &str =
    "w.id, w.slug, w.name, w.logo, w.members_can_create_projects, w.created_at, w.deleted_at";

#[derive(Debug, Default)]
pub struct WorkspacePatch {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub logo: Option<Option<String>>,
    pub members_can_create_projects: Option<bool>,
}

impl Store {
    /// Creates a workspace with `creator` as its first owner.
    pub async fn create_workspace(
        &self,
        slug: &str,
        name: &str,
        creator: Uuid,
    ) -> Result<MyWorkspace> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let id = new_id();
        let now = now_ts();
        sqlx::query(
            "INSERT INTO workspaces (id, slug, name, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(slug)
        .bind(name)
        .bind(creator)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(slug_taken)?;
        sqlx::query(
            "INSERT INTO workspace_members (workspace_id, user_id, role, created_at, updated_at) VALUES (?, ?, 'owner', ?, ?)",
        )
        .bind(id)
        .bind(creator)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(MyWorkspace {
            workspace: Workspace {
                id,
                slug: slug.into(),
                name: name.into(),
                logo: None,
                members_can_create_projects: false,
                created_at: now,
                deleted_at: None,
            },
            role: WsRole::Owner,
        })
    }

    /// Live workspaces the user is an active member of, by name.
    pub async fn my_workspaces(&self, user: Uuid) -> Result<Vec<MyWorkspace>> {
        let rows: Vec<WsColsAnd<String>> = sqlx::query_as(&format!(
            "SELECT {WS_COLS}, m.role FROM workspaces w
                 JOIN workspace_members m ON m.workspace_id = w.id
                 WHERE m.user_id = ? AND m.deactivated_at IS NULL AND w.deleted_at IS NULL
                 ORDER BY w.name COLLATE NOCASE"
        ))
        .bind(user)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        rows.into_iter().map(my_ws).collect()
    }

    /// The live workspace with this slug and the user's role; `not_found` for non-members, so
    /// a workspace's existence is not revealed.
    pub async fn workspace_for(&self, slug: &str, user: Uuid) -> Result<MyWorkspace> {
        let row: Option<WsColsAnd<String>> =
            sqlx::query_as(&format!(
                "SELECT {WS_COLS}, m.role FROM workspaces w
                 JOIN workspace_members m ON m.workspace_id = w.id
                 WHERE w.slug = ? AND m.user_id = ? AND m.deactivated_at IS NULL AND w.deleted_at IS NULL"
            ))
            .bind(slug)
            .bind(user)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
        row.map(my_ws)
            .transpose()?
            .ok_or_else(|| Error::not_found("workspace not found"))
    }

    pub async fn update_workspace(&self, id: Uuid, p: &WorkspacePatch) -> Result<()> {
        let now = now_ts();
        sqlx::query(
            "UPDATE workspaces SET
               name = COALESCE(?, name),
               slug = COALESCE(?, slug),
               logo = CASE WHEN ? THEN ? ELSE logo END,
               members_can_create_projects = COALESCE(?, members_can_create_projects),
               updated_at = ?
             WHERE id = ?",
        )
        .bind(&p.name)
        .bind(&p.slug)
        .bind(p.logo.is_some())
        .bind(p.logo.clone().flatten())
        .bind(p.members_can_create_projects)
        .bind(now)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(slug_taken)?;
        Ok(())
    }

    /// Moves a workspace to the trash. Its slug stays reserved until it is purged.
    pub async fn trash_workspace(&self, id: Uuid) -> Result<()> {
        let now = now_ts();
        sqlx::query("UPDATE workspaces SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Workspaces in the trash that the user owns.
    pub async fn trashed_workspaces(&self, user: Uuid) -> Result<Vec<Workspace>> {
        sqlx::query_as(&format!(
            "SELECT {WS_COLS} FROM workspaces w JOIN workspace_members m ON m.workspace_id = w.id
             WHERE m.user_id = ? AND m.role = 'owner' AND m.deactivated_at IS NULL AND w.deleted_at IS NOT NULL
             ORDER BY w.deleted_at DESC"
        ))
        .bind(user)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    /// Restores a trashed workspace the user owns.
    pub async fn restore_workspace(&self, slug: &str, user: Uuid) -> Result<()> {
        let r = sqlx::query(
            "UPDATE workspaces SET deleted_at = NULL, updated_at = ?
             WHERE slug = ? AND deleted_at IS NOT NULL AND id IN (
               SELECT workspace_id FROM workspace_members
               WHERE user_id = ? AND role = 'owner' AND deactivated_at IS NULL)",
        )
        .bind(now_ts())
        .bind(slug)
        .bind(user)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found(
                "no workspace with this slug in your trash",
            ));
        }
        Ok(())
    }

    /// Permanently removes workspaces trashed before `cutoff`. Returns how many.
    pub async fn purge_trashed_workspaces(&self, cutoff: &str) -> Result<u64> {
        let r =
            sqlx::query("DELETE FROM workspaces WHERE deleted_at IS NOT NULL AND deleted_at < ?")
                .bind(cutoff)
                .execute(self.pool())
                .await
                .map_err(db_err)?;
        Ok(r.rows_affected())
    }

    pub async fn workspace_members(&self, ws: Uuid) -> Result<Vec<WsMember>> {
        let rows: Vec<(Uuid, String, String, Option<String>, String, String)> = sqlx::query_as(
            "SELECT u.id, u.email, u.name, u.avatar, m.role, m.created_at
             FROM workspace_members m JOIN users u ON u.id = m.user_id
             WHERE m.workspace_id = ? AND m.deactivated_at IS NULL AND u.deleted_at IS NULL
             ORDER BY CASE m.role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 WHEN 'member' THEN 2 ELSE 3 END,
                      COALESCE(NULLIF(u.name, ''), u.email) COLLATE NOCASE",
        )
        .bind(ws)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        rows.into_iter()
            .map(|(user_id, email, name, avatar, role, joined_at)| {
                Ok(WsMember {
                    user_id,
                    email,
                    name,
                    avatar,
                    role: WsRole::parse(&role)?,
                    joined_at,
                })
            })
            .collect()
    }

    /// The user's active role in a workspace.
    pub async fn ws_role(&self, ws: Uuid, user: Uuid) -> Result<Option<WsRole>> {
        let r: Option<String> = sqlx::query_scalar(
            "SELECT role FROM workspace_members WHERE workspace_id = ? AND user_id = ? AND deactivated_at IS NULL",
        )
        .bind(ws)
        .bind(user)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        r.map(|r| WsRole::parse(&r)).transpose()
    }

    pub async fn count_owners(&self, ws: Uuid) -> Result<usize> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workspace_members WHERE workspace_id = ? AND role = 'owner' AND deactivated_at IS NULL",
        )
        .bind(ws)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(n as usize)
    }

    pub async fn set_ws_role(&self, ws: Uuid, user: Uuid, role: WsRole) -> Result<()> {
        sqlx::query("UPDATE workspace_members SET role = ?, updated_at = ? WHERE workspace_id = ? AND user_id = ? AND deactivated_at IS NULL")
            .bind(role.as_str())
            .bind(now_ts())
            .bind(ws)
            .bind(user)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Ends a membership (removal or leaving). The row stays, deactivated.
    pub async fn deactivate_ws_member(&self, ws: Uuid, user: Uuid) -> Result<()> {
        let now = now_ts();
        sqlx::query("UPDATE workspace_members SET deactivated_at = ?, updated_at = ? WHERE workspace_id = ? AND user_id = ? AND deactivated_at IS NULL")
            .bind(&now)
            .bind(&now)
            .bind(ws)
            .bind(user)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn all_workspaces(&self) -> Result<Vec<(Workspace, i64)>> {
        let rows: Vec<WsColsAnd<i64>> = sqlx::query_as(&format!(
            "SELECT {WS_COLS}, (SELECT COUNT(*) FROM workspace_members m WHERE m.workspace_id = w.id AND m.deactivated_at IS NULL)
             FROM workspaces w ORDER BY w.name COLLATE NOCASE"
        ))
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(|(id, slug, name, logo, m, created_at, deleted_at, n)| {
                (
                    Workspace {
                        id,
                        slug,
                        name,
                        logo,
                        members_can_create_projects: m,
                        created_at,
                        deleted_at,
                    },
                    n,
                )
            })
            .collect())
    }
}

/// Adds (or reactivates) a membership inside a transaction.
pub(crate) async fn upsert_member(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ws: Uuid,
    user: Uuid,
    role: WsRole,
) -> Result<()> {
    let now = now_ts();
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (workspace_id, user_id) DO UPDATE SET
           role = CASE WHEN deactivated_at IS NULL THEN role ELSE excluded.role END,
           deactivated_at = NULL, updated_at = excluded.updated_at",
    )
    .bind(ws)
    .bind(user)
    .bind(role.as_str())
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .map_err(db_err)?;
    Ok(())
}

type WsRow = (
    Uuid,
    String,
    String,
    Option<String>,
    bool,
    String,
    Option<String>,
    String,
);

fn my_ws(r: WsRow) -> Result<MyWorkspace> {
    let (id, slug, name, logo, members_can_create_projects, created_at, deleted_at, role) = r;
    Ok(MyWorkspace {
        workspace: Workspace {
            id,
            slug,
            name,
            logo,
            members_can_create_projects,
            created_at,
            deleted_at,
        },
        role: WsRole::parse(&role)?,
    })
}

fn slug_taken(e: sqlx::Error) -> Error {
    match db_err(e) {
        e if e.kind == np_core::ErrorKind::Conflict => {
            Error::conflict("this workspace URL is taken")
        }
        e => e,
    }
}

impl Store {
    /// How many workspaces (trashed included) use a slug.
    pub async fn slug_count(&self, slug: &str) -> Result<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM workspaces WHERE slug = ?")
            .bind(slug)
            .fetch_one(self.pool())
            .await
            .map_err(db_err)
    }
}

impl Store {
    /// Refuses when the user is the only explicit admin of some project in the workspace
    /// (PERMISSIONS §1).
    pub async fn check_not_sole_project_admin(&self, ws: Uuid, user: Uuid) -> Result<()> {
        let name: Option<String> = sqlx::query_scalar(
            "SELECT p.name FROM projects p JOIN project_members pm ON pm.project_id = p.id
             WHERE p.workspace_id = ? AND p.deleted_at IS NULL AND pm.user_id = ? AND pm.role = 'admin'
               AND NOT EXISTS (
                 SELECT 1 FROM project_members o
                 JOIN workspace_members wm ON wm.workspace_id = p.workspace_id AND wm.user_id = o.user_id AND wm.deactivated_at IS NULL
                 WHERE o.project_id = p.id AND o.role = 'admin' AND o.user_id <> pm.user_id)
             LIMIT 1",
        )
        .bind(ws)
        .bind(user)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        match name {
            Some(n) => Err(Error::conflict(format!(
                "they are the only admin of project \"{n}\"; add another project admin first"
            ))),
            None => Ok(()),
        }
    }
}

impl Store {
    pub async fn workspace_slug(&self, id: Uuid) -> Result<Option<String>> {
        sqlx::query_scalar("SELECT slug FROM workspaces WHERE id = ? AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)
    }
}

/// A workspace's columns (`WS_COLS`) followed by one more value (the member's role, or a count).
type WsColsAnd<T> = (
    Uuid,
    String,
    String,
    Option<String>,
    bool,
    String,
    Option<String>,
    T,
);
