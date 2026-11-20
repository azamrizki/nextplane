//! Modules: feature-sized groups of work items with a lead, members, status and dates.

use np_core::id::{Uuid, new_id};
use np_core::{Error, ErrorKind, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::cycles::{Progress, ProgressRow, progress_of};
use crate::{Store, db_err, now_ts};

pub const STATUSES: [&str; 6] = [
    "backlog",
    "planned",
    "in_progress",
    "paused",
    "completed",
    "cancelled",
];

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Module {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: String,
    pub status: String,
    pub lead_id: Option<Uuid>,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub created_by: Uuid,
    pub created_at: String,
    pub archived_at: Option<String>,
    pub total_items: i64,
    pub completed_items: i64,
    #[sqlx(skip)]
    pub member_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ModuleLink {
    pub id: Uuid,
    pub url: String,
    pub title: String,
    pub created_by: Uuid,
    pub created_at: String,
}

#[derive(Debug, Default, Clone)]
pub struct ModuleFields {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub lead_id: Option<Option<Uuid>>,
    pub dates: Option<(Option<String>, Option<String>)>,
    pub member_ids: Option<Vec<Uuid>>,
}

const COLS: &str = "m.id, m.project_id, m.name, m.description, m.status, m.lead_id, m.start_date, m.target_date, m.created_by, m.created_at, m.archived_at,
    (SELECT COUNT(*) FROM work_item_modules x JOIN work_items w ON w.id = x.work_item_id WHERE x.module_id = m.id AND w.deleted_at IS NULL AND w.is_draft = 0) AS total_items,
    (SELECT COUNT(*) FROM work_item_modules x JOIN work_items w ON w.id = x.work_item_id JOIN states s ON s.id = w.state_id
       WHERE x.module_id = m.id AND w.deleted_at IS NULL AND w.is_draft = 0 AND s.grp IN ('completed', 'cancelled')) AS completed_items";

fn name_taken(e: sqlx::Error) -> Error {
    match db_err(e) {
        e if e.kind == ErrorKind::Conflict => {
            Error::conflict("a module with this name already exists")
        }
        e => e,
    }
}

impl Store {
    async fn with_members(&self, mut ms: Vec<Module>) -> Result<Vec<Module>> {
        for m in &mut ms {
            m.member_ids =
                sqlx::query_scalar("SELECT user_id FROM module_members WHERE module_id = ?")
                    .bind(m.id)
                    .fetch_all(self.pool())
                    .await
                    .map_err(db_err)?;
        }
        Ok(ms)
    }

    async fn set_members(&self, id: Uuid, members: &[Uuid]) -> Result<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        sqlx::query("DELETE FROM module_members WHERE module_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        for u in members {
            sqlx::query("INSERT OR IGNORE INTO module_members (module_id, user_id) VALUES (?, ?)")
                .bind(id)
                .bind(u)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)
    }

    pub async fn create_module(&self, project: Uuid, f: &ModuleFields, by: Uuid) -> Result<Module> {
        let id = new_id();
        let now = now_ts();
        let (s, e) = f.dates.clone().unwrap_or((None, None));
        sqlx::query(
            "INSERT INTO modules (id, project_id, name, description, status, lead_id, start_date, target_date, created_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(project)
        .bind(f.name.as_deref().unwrap_or(""))
        .bind(f.description.as_deref().unwrap_or(""))
        .bind(f.status.as_deref().unwrap_or("backlog"))
        .bind(f.lead_id.flatten())
        .bind(s)
        .bind(e)
        .bind(by)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(name_taken)?;
        if let Some(ms) = &f.member_ids {
            self.set_members(id, ms).await?;
        }
        self.module(project, id)
            .await?
            .ok_or_else(|| Error::not_found("module not found"))
    }

    pub async fn module(&self, project: Uuid, id: Uuid) -> Result<Option<Module>> {
        let m: Option<Module> = sqlx::query_as(&format!("SELECT {COLS} FROM modules m WHERE m.id = ? AND m.project_id = ? AND m.deleted_at IS NULL"))
            .bind(id)
            .bind(project)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
        match m {
            Some(m) => Ok(self.with_members(vec![m]).await?.pop()),
            None => Ok(None),
        }
    }

    pub async fn modules(&self, project: Uuid, archived: bool) -> Result<Vec<Module>> {
        let ms: Vec<Module> = sqlx::query_as(&format!(
            "SELECT {COLS} FROM modules m WHERE m.project_id = ? AND m.deleted_at IS NULL AND (m.archived_at IS NOT NULL) = ?
             ORDER BY m.name COLLATE NOCASE"
        ))
        .bind(project)
        .bind(archived)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        self.with_members(ms).await
    }

    pub async fn update_module(&self, project: Uuid, id: Uuid, f: &ModuleFields) -> Result<Module> {
        let cur = self
            .module(project, id)
            .await?
            .ok_or_else(|| Error::not_found("module not found"))?;
        let (s, e) = f
            .dates
            .clone()
            .unwrap_or((cur.start_date.clone(), cur.target_date.clone()));
        sqlx::query(
            "UPDATE modules SET name = COALESCE(?, name), description = COALESCE(?, description), status = COALESCE(?, status),
               lead_id = CASE WHEN ? THEN ? ELSE lead_id END, start_date = ?, target_date = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&f.name)
        .bind(&f.description)
        .bind(&f.status)
        .bind(f.lead_id.is_some())
        .bind(f.lead_id.flatten())
        .bind(s)
        .bind(e)
        .bind(now_ts())
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(name_taken)?;
        if let Some(ms) = &f.member_ids {
            self.set_members(id, ms).await?;
        }
        self.module(project, id)
            .await?
            .ok_or_else(|| Error::not_found("module not found"))
    }

    pub async fn set_module_archived(&self, id: Uuid, on: bool) -> Result<()> {
        let now = now_ts();
        sqlx::query("UPDATE modules SET archived_at = CASE WHEN ? THEN ? ELSE NULL END, updated_at = ? WHERE id = ?")
            .bind(on)
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Deletes a module; its work items stay (they just leave the module).
    pub async fn delete_module(&self, id: Uuid) -> Result<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let now = now_ts();
        sqlx::query("DELETE FROM work_item_modules WHERE module_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("UPDATE modules SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }

    pub async fn module_progress(&self, m: &Module, estimate_numeric: bool) -> Result<Progress> {
        let rows: Vec<ProgressRow> = sqlx::query_as(
            "SELECT s.grp, w.estimate, w.completed_at FROM work_item_modules x JOIN work_items w ON w.id = x.work_item_id
             JOIN states s ON s.id = w.state_id WHERE x.module_id = ? AND w.deleted_at IS NULL AND w.is_draft = 0",
        )
        .bind(m.id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        Ok(progress_of(
            &rows,
            m.start_date.as_deref(),
            m.target_date.as_deref(),
            estimate_numeric,
        ))
    }

    pub async fn module_links(&self, id: Uuid) -> Result<Vec<ModuleLink>> {
        sqlx::query_as("SELECT id, url, title, created_by, created_at FROM module_links WHERE module_id = ? ORDER BY created_at")
            .bind(id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }

    pub async fn add_module_link(&self, id: Uuid, url: &str, title: &str, by: Uuid) -> Result<()> {
        sqlx::query("INSERT INTO module_links (id, module_id, url, title, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(new_id())
            .bind(id)
            .bind(url)
            .bind(title)
            .bind(by)
            .bind(now_ts())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn remove_module_link(&self, id: Uuid, link: Uuid) -> Result<()> {
        let r = sqlx::query("DELETE FROM module_links WHERE id = ? AND module_id = ?")
            .bind(link)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("link not found"));
        }
        Ok(())
    }
}
