//! Numbers and activity for a member's profile page (S3.7). The caller passes the projects
//! the *viewer* may read, so two people can see different numbers for the same profile, as
//! in Plane.

use np_core::Result;
use np_core::id::Uuid;
use serde::Serialize;
use sqlx::{FromRow, QueryBuilder, Sqlite};

use crate::{Store, db_err};

#[derive(Debug, Clone, Default, Serialize, FromRow)]
pub struct ProjectNumbers {
    pub project_id: Uuid,
    pub created: i64,
    pub assigned: i64,
    /// Assigned and still open (backlog, unstarted, started).
    pub pending: i64,
    /// Assigned and completed.
    pub completed: i64,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Count {
    pub key: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ActivityEntry {
    pub id: Uuid,
    pub verb: String,
    pub field: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub created_at: String,
    pub work_item_id: Uuid,
    pub project_id: Uuid,
    pub sequence: i64,
    pub title: String,
}

fn in_projects(qb: &mut QueryBuilder<'_, Sqlite>, projects: &[Uuid]) {
    qb.push(" w.project_id IN (");
    let mut sep = qb.separated(", ");
    for p in projects {
        sep.push_bind(*p);
    }
    qb.push(")");
}

const LIVE: &str = " AND w.deleted_at IS NULL AND w.archived_at IS NULL AND w.is_draft = 0";

impl Store {
    /// Per project: what `user` created, is assigned, and of that, what is open or completed.
    pub async fn profile_numbers(&self, projects: &[Uuid], user: Uuid) -> Result<Vec<ProjectNumbers>> {
        if projects.is_empty() {
            return Ok(Vec::new());
        }
        let mut qb = QueryBuilder::<Sqlite>::new(
            "SELECT w.project_id,
               SUM(w.created_by = ",
        );
        qb.push_bind(user);
        qb.push(") AS created, SUM(a.user_id IS NOT NULL) AS assigned,
               SUM(a.user_id IS NOT NULL AND s.grp IN ('backlog', 'unstarted', 'started')) AS pending,
               SUM(a.user_id IS NOT NULL AND s.grp = 'completed') AS completed
             FROM work_items w JOIN states s ON s.id = w.state_id
             LEFT JOIN work_item_assignees a ON a.work_item_id = w.id AND a.user_id = ");
        qb.push_bind(user);
        qb.push(" WHERE");
        in_projects(&mut qb, projects);
        qb.push(LIVE);
        qb.push(" GROUP BY w.project_id");
        qb.build_query_as()
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }

    /// Items `user` follows in those projects.
    pub async fn profile_subscribed(&self, projects: &[Uuid], user: Uuid) -> Result<i64> {
        if projects.is_empty() {
            return Ok(0);
        }
        let mut qb = QueryBuilder::<Sqlite>::new(
            "SELECT COUNT(*) FROM work_items w JOIN subscriptions x ON x.work_item_id = w.id AND x.user_id = ",
        );
        qb.push_bind(user);
        qb.push(" WHERE");
        in_projects(&mut qb, projects);
        qb.push(LIVE);
        qb.build_query_scalar()
            .fetch_one(self.pool())
            .await
            .map_err(db_err)
    }

    /// Items assigned to `user`, counted by `state group` or by `priority`.
    pub async fn profile_assigned_by(&self, projects: &[Uuid], user: Uuid, by_priority: bool) -> Result<Vec<Count>> {
        if projects.is_empty() {
            return Ok(Vec::new());
        }
        let col = if by_priority { "w.priority" } else { "s.grp" };
        let mut qb = QueryBuilder::<Sqlite>::new(format!(
            "SELECT {col} AS key, COUNT(*) AS count FROM work_items w JOIN states s ON s.id = w.state_id
             JOIN work_item_assignees a ON a.work_item_id = w.id AND a.user_id = "
        ));
        qb.push_bind(user);
        qb.push(" WHERE");
        in_projects(&mut qb, projects);
        qb.push(LIVE);
        qb.push(format!(" GROUP BY {col}"));
        qb.build_query_as()
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }

    /// What `user` did to work items in those projects, newest first: pages of `limit` before
    /// a (created_at, id) cursor, or one UTC day (`YYYY-MM-DD`).
    pub async fn user_activity(
        &self,
        projects: &[Uuid],
        user: Uuid,
        before: Option<(String, Uuid)>,
        day: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ActivityEntry>> {
        if projects.is_empty() {
            return Ok(Vec::new());
        }
        let mut qb = QueryBuilder::<Sqlite>::new(
            "SELECT a.id, a.verb, a.field, a.old_value, a.new_value, a.created_at, w.id AS work_item_id,
               w.project_id, w.sequence, w.title
             FROM work_item_activity a JOIN work_items w ON w.id = a.work_item_id
             WHERE a.actor_id = ",
        );
        qb.push_bind(user);
        qb.push(" AND");
        in_projects(&mut qb, projects);
        qb.push(" AND w.deleted_at IS NULL AND w.is_draft = 0");
        if let Some(d) = day {
            qb.push(" AND a.created_at LIKE ").push_bind(format!("{d}%"));
        }
        if let Some((at, id)) = before {
            qb.push(" AND (a.created_at < ")
                .push_bind(at.clone())
                .push(" OR (a.created_at = ")
                .push_bind(at)
                .push(" AND a.id < ")
                .push_bind(id)
                .push("))");
        }
        qb.push(" ORDER BY a.created_at DESC, a.id DESC LIMIT ").push_bind(limit);
        qb.build_query_as()
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }
}
