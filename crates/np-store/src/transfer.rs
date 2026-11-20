//! Export and import of a project's work items (SPRINT S3.5).

use np_core::Result;
use np_core::id::Uuid;
use serde::Serialize;
use sqlx::FromRow;

use crate::{Store, db_err};

/// One work item as exported: names instead of ids.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ExportRow {
    pub key: String,
    pub title: String,
    pub state: String,
    pub state_group: String,
    pub priority: String,
    pub assignees: String,
    pub labels: String,
    pub cycle: Option<String>,
    pub modules: String,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub estimate: Option<String>,
    pub parent: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub archived_at: Option<String>,
    pub description_html: String,
}

impl Store {
    /// Every live work item of the project (archived ones too), oldest first. Lists inside a
    /// field are joined with "; ".
    pub async fn export_items(&self, project: Uuid) -> Result<Vec<ExportRow>> {
        sqlx::query_as(
            "SELECT p.identifier || '-' || w.sequence AS key, w.title, s.name AS state, s.grp AS state_group, w.priority,
               COALESCE((SELECT group_concat(u.email, '; ') FROM work_item_assignees a JOIN users u ON u.id = a.user_id WHERE a.work_item_id = w.id), '') AS assignees,
               COALESCE((SELECT group_concat(l.name, '; ') FROM work_item_labels wl JOIN labels l ON l.id = wl.label_id WHERE wl.work_item_id = w.id), '') AS labels,
               c.name AS cycle,
               COALESCE((SELECT group_concat(m.name, '; ') FROM work_item_modules wm JOIN modules m ON m.id = wm.module_id AND m.deleted_at IS NULL WHERE wm.work_item_id = w.id), '') AS modules,
               w.start_date, w.due_date, w.estimate,
               CASE WHEN par.id IS NULL THEN NULL ELSE p.identifier || '-' || par.sequence END AS parent,
               cu.email AS created_by, w.created_at, w.updated_at, w.completed_at, w.archived_at, w.description_html
             FROM work_items w
             JOIN projects p ON p.id = w.project_id
             JOIN states s ON s.id = w.state_id
             JOIN users cu ON cu.id = w.created_by
             LEFT JOIN cycles c ON c.id = w.cycle_id AND c.deleted_at IS NULL
             LEFT JOIN work_items par ON par.id = w.parent_id AND par.deleted_at IS NULL
             WHERE w.project_id = ? AND w.deleted_at IS NULL AND w.is_draft = 0
             ORDER BY w.sequence",
        )
        .bind(project)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }
}
