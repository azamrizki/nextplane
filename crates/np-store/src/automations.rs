//! Project automations (S3.7): which projects have them, and which work items they apply to.

use np_core::id::Uuid;
use np_core::Result;
use sqlx::{FromRow, QueryBuilder, Sqlite};

use crate::{Store, db_err};

#[derive(Debug, Clone, FromRow)]
pub struct AutomatedProject {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub slug: String,
    pub archive_in: i64,
    pub close_in: i64,
    pub close_state_id: Option<Uuid>,
}

impl Store {
    /// Live projects (in live workspaces) with an automation turned on.
    pub async fn automated_projects(&self) -> Result<Vec<AutomatedProject>> {
        sqlx::query_as(
            "SELECT p.id, p.workspace_id, w.slug, p.archive_in, p.close_in, p.close_state_id
             FROM projects p JOIN workspaces w ON w.id = p.workspace_id
             WHERE p.deleted_at IS NULL AND p.archived_at IS NULL AND w.deleted_at IS NULL
               AND (p.archive_in > 0 OR p.close_in > 0)
             ORDER BY p.created_at",
        )
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    /// Published, unarchived work items of `project` in one of `groups`, unchanged since
    /// `before` (a timestamp), oldest first. Items in a cycle that has not ended, or in a module
    /// whose target date has not passed (or that has none), are left alone, as in Plane.
    pub async fn automation_candidates(
        &self,
        project: Uuid,
        groups: &[&str],
        before: &str,
        today: &str,
        limit: i64,
    ) -> Result<Vec<Uuid>> {
        let mut qb = QueryBuilder::<Sqlite>::new(
            "SELECT w.id FROM work_items w JOIN states s ON s.id = w.state_id
             WHERE w.project_id = ",
        );
        qb.push_bind(project);
        qb.push(" AND w.deleted_at IS NULL AND w.archived_at IS NULL AND w.is_draft = 0 AND s.grp IN (");
        let mut sep = qb.separated(", ");
        for g in groups {
            sep.push_bind(g.to_string());
        }
        qb.push(") AND w.updated_at < ").push_bind(before.to_string());
        qb.push(
            " AND (w.cycle_id IS NULL OR EXISTS (SELECT 1 FROM cycles c WHERE c.id = w.cycle_id
                   AND c.end_date IS NOT NULL AND c.end_date < ",
        )
        .push_bind(today.to_string());
        qb.push(
            ")) AND NOT EXISTS (SELECT 1 FROM work_item_modules x JOIN modules m ON m.id = x.module_id
                 AND m.deleted_at IS NULL WHERE x.work_item_id = w.id AND (m.target_date IS NULL OR m.target_date >= ",
        )
        .push_bind(today.to_string());
        qb.push(")) ORDER BY w.updated_at LIMIT ").push_bind(limit);
        qb.build_query_scalar()
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }
}
