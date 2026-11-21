//! Saved views and "My work" across projects.

use np_core::id::{Uuid, new_id};
use np_core::{Error, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::work_items::{Filter, WorkItem};
use crate::{Store, db_err, now_ts};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct View {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub name: String,
    pub description: String,
    pub is_shared: bool,
    #[sqlx(json)]
    pub config: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

const COLS: &str =
    "id, project_id, owner_id, name, description, is_shared, config, created_at, updated_at";

/// A work item with its project's identifier, for lists that span projects.
#[derive(Debug, Clone, Serialize)]
pub struct WorkItemInProject {
    #[serde(flatten)]
    pub item: WorkItem,
    pub identifier: String,
    pub project_name: String,
}

impl Store {
    /// Views of a project the user may see: shared ones and their own.
    pub async fn project_views(&self, project: Uuid, user: Uuid) -> Result<Vec<View>> {
        sqlx::query_as(&format!(
            "SELECT {COLS} FROM views WHERE project_id = ? AND deleted_at IS NULL AND (is_shared = 1 OR owner_id = ?)
             ORDER BY name COLLATE NOCASE"
        ))
        .bind(project)
        .bind(user)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn view(&self, project: Uuid, id: Uuid) -> Result<Option<View>> {
        sqlx::query_as(&format!(
            "SELECT {COLS} FROM views WHERE id = ? AND project_id = ? AND deleted_at IS NULL"
        ))
        .bind(id)
        .bind(project)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn create_view(
        &self,
        ws: Uuid,
        project: Uuid,
        owner: Uuid,
        name: &str,
        shared: bool,
        config: &serde_json::Value,
    ) -> Result<View> {
        let id = new_id();
        let now = now_ts();
        sqlx::query(
            "INSERT INTO views (id, workspace_id, project_id, owner_id, name, is_shared, config, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(ws)
        .bind(project)
        .bind(owner)
        .bind(name)
        .bind(shared)
        .bind(config.to_string())
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.view(project, id)
            .await?
            .ok_or_else(|| Error::not_found("view not found"))
    }

    pub async fn update_view(
        &self,
        project: Uuid,
        id: Uuid,
        name: Option<&str>,
        shared: Option<bool>,
        config: Option<&serde_json::Value>,
    ) -> Result<View> {
        sqlx::query(
            "UPDATE views SET name = COALESCE(?, name), is_shared = COALESCE(?, is_shared), config = COALESCE(?, config), updated_at = ? WHERE id = ?",
        )
        .bind(name)
        .bind(shared)
        .bind(config.map(|c| c.to_string()))
        .bind(now_ts())
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.view(project, id)
            .await?
            .ok_or_else(|| Error::not_found("view not found"))
    }

    /// Workspace views (no project) the user may see: shared ones and their own, or only their
    /// own (workspace guests, as in Plane).
    pub async fn workspace_views(&self, ws: Uuid, user: Uuid, own_only: bool) -> Result<Vec<View>> {
        sqlx::query_as(&format!(
            "SELECT {COLS} FROM views WHERE workspace_id = ? AND project_id IS NULL AND deleted_at IS NULL
               AND (owner_id = ? OR (is_shared = 1 AND NOT ?))
             ORDER BY created_at, id"
        ))
        .bind(ws)
        .bind(user)
        .bind(own_only)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn workspace_view(&self, ws: Uuid, id: Uuid) -> Result<Option<View>> {
        sqlx::query_as(&format!(
            "SELECT {COLS} FROM views WHERE id = ? AND workspace_id = ? AND project_id IS NULL AND deleted_at IS NULL"
        ))
        .bind(id)
        .bind(ws)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn create_workspace_view(
        &self,
        ws: Uuid,
        owner: Uuid,
        name: &str,
        description: &str,
        shared: bool,
        config: &serde_json::Value,
    ) -> Result<View> {
        let id = new_id();
        let now = now_ts();
        sqlx::query(
            "INSERT INTO views (id, workspace_id, project_id, owner_id, name, description, is_shared, config, created_at, updated_at)
             VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(ws)
        .bind(owner)
        .bind(name)
        .bind(description)
        .bind(shared)
        .bind(config.to_string())
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.workspace_view(ws, id)
            .await?
            .ok_or_else(|| Error::not_found("view not found"))
    }

    pub async fn update_workspace_view(
        &self,
        ws: Uuid,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        shared: Option<bool>,
        config: Option<&serde_json::Value>,
    ) -> Result<View> {
        sqlx::query(
            "UPDATE views SET name = COALESCE(?, name), description = COALESCE(?, description),
               is_shared = COALESCE(?, is_shared), config = COALESCE(?, config), updated_at = ?
             WHERE id = ? AND workspace_id = ?",
        )
        .bind(name)
        .bind(description)
        .bind(shared)
        .bind(config.map(|c| c.to_string()))
        .bind(now_ts())
        .bind(id)
        .bind(ws)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.workspace_view(ws, id)
            .await?
            .ok_or_else(|| Error::not_found("view not found"))
    }

    pub async fn delete_view(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE views SET deleted_at = ? WHERE id = ?")
            .bind(now_ts())
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Open (or all) work items across `projects` that the user is assigned to, created, or
    /// follows. The caller passes only projects the user may read.
    pub async fn my_work(
        &self,
        projects: &[(Uuid, String, String)],
        user: Uuid,
        kind: &str,
        include_closed: bool,
    ) -> Result<Vec<WorkItemInProject>> {
        let mut out = Vec::new();
        for (pid, identifier, name) in projects {
            let mut f = Filter {
                limit: Some(500),
                ..Default::default()
            };
            match kind {
                "assigned" => f.assignee_ids = vec![user],
                "created" => f.created_by = vec![user],
                "subscribed" => f.subscribed_by = Some(user),
                _ => {
                    return Err(Error::bad_request(
                        "kind is assigned, created or subscribed",
                    ));
                }
            }
            if !include_closed {
                f.state_groups = vec!["backlog".into(), "unstarted".into(), "started".into()];
            }
            for item in self.work_items(*pid, &f).await? {
                out.push(WorkItemInProject {
                    item,
                    identifier: identifier.clone(),
                    project_name: name.clone(),
                });
            }
        }
        out.sort_by(|a, b| b.item.updated_at.cmp(&a.item.updated_at));
        Ok(out)
    }
}
