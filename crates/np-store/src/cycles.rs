//! Cycles (sprints): CRUD, progress, burn-down and transfer of unfinished items.

use np_core::cycle::{self, CycleStatus};
use np_core::id::{Uuid, new_id};
use np_core::{Error, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::{Store, db_err, now_ts};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Cycle {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub owner_id: Option<Uuid>,
    pub created_by: Uuid,
    pub created_at: String,
    pub archived_at: Option<String>,
    #[sqlx(skip)]
    pub status: Option<CycleStatus>,
    pub total_items: i64,
    pub completed_items: i64,
}

const COLS: &str = "c.id, c.project_id, c.name, c.description, c.start_date, c.end_date, c.owner_id, c.created_by, c.created_at, c.archived_at,
    (SELECT COUNT(*) FROM work_items w WHERE w.cycle_id = c.id AND w.deleted_at IS NULL AND w.is_draft = 0) AS total_items,
    (SELECT COUNT(*) FROM work_items w JOIN states s ON s.id = w.state_id
       WHERE w.cycle_id = c.id AND w.deleted_at IS NULL AND w.is_draft = 0 AND s.grp IN ('completed', 'cancelled')) AS completed_items";

#[derive(Debug, Default, Clone)]
pub struct CyclePatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub dates: Option<(Option<String>, Option<String>)>,
    pub owner_id: Option<Option<Uuid>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupCount {
    pub group: String,
    pub items: i64,
    pub points: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BurnPoint {
    pub date: String,
    /// Straight line from the starting scope to zero.
    pub ideal: f64,
    /// Items still open at the end of that day; `null` for days still to come.
    pub remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub groups: Vec<GroupCount>,
    pub total_items: i64,
    pub completed_items: i64,
    pub total_points: f64,
    pub completed_points: f64,
    pub burndown: Vec<BurnPoint>,
}

fn today() -> String {
    now_ts()[..10].to_string()
}

fn with_status(mut c: Cycle) -> Cycle {
    c.status = Some(cycle::status(
        c.start_date.as_deref(),
        c.end_date.as_deref(),
        &today(),
    ));
    c
}

impl Store {
    async fn check_overlap(
        &self,
        project: Uuid,
        id: Option<Uuid>,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<()> {
        let (Some(s), Some(e)) = (start, end) else {
            return Ok(());
        };
        if s > e {
            return Err(Error::bad_request(
                "the start date must not be after the end date",
            ));
        }
        let others: Vec<(Uuid, String, String, String)> = sqlx::query_as(
            "SELECT id, name, start_date, end_date FROM cycles
             WHERE project_id = ? AND deleted_at IS NULL AND start_date IS NOT NULL AND end_date IS NOT NULL",
        )
        .bind(project)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        for (oid, name, os, oe) in others {
            if Some(oid) != id && cycle::overlaps((s, e), (&os, &oe)) {
                return Err(Error::conflict(format!(
                    "these dates overlap cycle \"{name}\""
                )));
            }
        }
        Ok(())
    }

    pub async fn create_cycle(
        &self,
        project: Uuid,
        name: &str,
        description: &str,
        start: Option<&str>,
        end: Option<&str>,
        owner: Option<Uuid>,
        by: Uuid,
    ) -> Result<Cycle> {
        self.check_overlap(project, None, start, end).await?;
        let id = new_id();
        let now = now_ts();
        sqlx::query(
            "INSERT INTO cycles (id, project_id, name, description, start_date, end_date, owner_id, created_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(project)
        .bind(name)
        .bind(description)
        .bind(start)
        .bind(end)
        .bind(owner)
        .bind(by)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.cycle(project, id)
            .await?
            .ok_or_else(|| Error::not_found("cycle not found"))
    }

    pub async fn cycle(&self, project: Uuid, id: Uuid) -> Result<Option<Cycle>> {
        let c: Option<Cycle> = sqlx::query_as(&format!("SELECT {COLS} FROM cycles c WHERE c.id = ? AND c.project_id = ? AND c.deleted_at IS NULL"))
            .bind(id)
            .bind(project)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
        Ok(c.map(with_status))
    }

    pub async fn cycles(&self, project: Uuid, archived: bool) -> Result<Vec<Cycle>> {
        let cs: Vec<Cycle> = sqlx::query_as(&format!(
            "SELECT {COLS} FROM cycles c WHERE c.project_id = ? AND c.deleted_at IS NULL AND (c.archived_at IS NOT NULL) = ?
             ORDER BY c.start_date IS NULL, c.start_date DESC, c.created_at DESC"
        ))
        .bind(project)
        .bind(archived)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        Ok(cs.into_iter().map(with_status).collect())
    }

    pub async fn update_cycle(&self, project: Uuid, id: Uuid, p: &CyclePatch) -> Result<Cycle> {
        let cur = self
            .cycle(project, id)
            .await?
            .ok_or_else(|| Error::not_found("cycle not found"))?;
        if let Some((s, e)) = &p.dates {
            self.check_overlap(project, Some(id), s.as_deref(), e.as_deref())
                .await?;
        }
        let (s, e) = p
            .dates
            .clone()
            .unwrap_or((cur.start_date.clone(), cur.end_date.clone()));
        sqlx::query(
            "UPDATE cycles SET name = COALESCE(?, name), description = COALESCE(?, description), start_date = ?, end_date = ?,
               owner_id = CASE WHEN ? THEN ? ELSE owner_id END, updated_at = ? WHERE id = ?",
        )
        .bind(&p.name)
        .bind(&p.description)
        .bind(s)
        .bind(e)
        .bind(p.owner_id.is_some())
        .bind(p.owner_id.flatten())
        .bind(now_ts())
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.cycle(project, id)
            .await?
            .ok_or_else(|| Error::not_found("cycle not found"))
    }

    pub async fn set_cycle_archived(&self, id: Uuid, on: bool) -> Result<()> {
        let now = now_ts();
        sqlx::query("UPDATE cycles SET archived_at = CASE WHEN ? THEN ? ELSE NULL END, updated_at = ? WHERE id = ?")
            .bind(on)
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Deletes a cycle; its items stay in the project without a cycle.
    pub async fn delete_cycle(&self, id: Uuid) -> Result<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let now = now_ts();
        sqlx::query("UPDATE work_items SET cycle_id = NULL, updated_at = ? WHERE cycle_id = ?")
            .bind(&now)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("UPDATE cycles SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }

    /// Counts and points per state group, and a burn-down by day.
    pub async fn cycle_progress(&self, c: &Cycle, estimate_numeric: bool) -> Result<Progress> {
        let rows: Vec<ProgressRow> = sqlx::query_as(
            "SELECT s.grp, w.estimate, w.completed_at FROM work_items w JOIN states s ON s.id = w.state_id
             WHERE w.cycle_id = ? AND w.deleted_at IS NULL AND w.is_draft = 0",
        )
        .bind(c.id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        Ok(progress_of(
            &rows,
            c.start_date.as_deref(),
            c.end_date.as_deref(),
            estimate_numeric,
        ))
    }

    /// Moves the unfinished items of `from` to `to`; returns how many moved.
    pub async fn transfer_cycle(
        &self,
        project: Uuid,
        from: Uuid,
        to: Uuid,
        actor: Uuid,
    ) -> Result<u64> {
        if from == to {
            return Err(Error::bad_request("choose another cycle"));
        }
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT w.id FROM work_items w JOIN states s ON s.id = w.state_id
             WHERE w.cycle_id = ? AND w.deleted_at IS NULL AND w.is_draft = 0 AND s.grp NOT IN ('completed', 'cancelled')",
        )
        .bind(from)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        let patch = crate::work_items::WorkItemPatch {
            cycle_id: Some(Some(to)),
            ..Default::default()
        };
        for id in &ids {
            self.update_work_item(project, *id, &patch, actor).await?;
        }
        Ok(ids.len() as u64)
    }
}

/// (state group, estimate, completed_at) of one work item.
pub type ProgressRow = (String, Option<String>, Option<String>);

/// Progress of a set of work items, with a burn-down between `start` and `end` if both are set.
pub fn progress_of(
    rows: &[ProgressRow],
    start: Option<&str>,
    end: Option<&str>,
    estimate_numeric: bool,
) -> Progress {
    let pts = |e: &Option<String>| {
        if estimate_numeric {
            e.as_deref()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0)
        } else {
            0.0
        }
    };
    let closed = |g: &str| g == "completed" || g == "cancelled";
    let mut groups: Vec<GroupCount> = np_core::project::StateGroup::ALL
        .iter()
        .map(|g| GroupCount {
            group: g.as_str().into(),
            items: 0,
            points: 0.0,
        })
        .collect();
    let (mut total_points, mut completed_points) = (0.0, 0.0);
    for (g, e, _) in rows {
        if let Some(gc) = groups.iter_mut().find(|x| &x.group == g) {
            gc.items += 1;
            gc.points += pts(e);
        }
        total_points += pts(e);
        if closed(g) {
            completed_points += pts(e);
        }
    }
    let total = rows.len() as i64;
    let done = rows.iter().filter(|r| closed(&r.0)).count() as i64;
    let mut burndown = Vec::new();
    if let (Some(s), Some(e)) = (start, end) {
        let days = cycle::days(s, e);
        let n = days.len().max(1) as f64;
        let today = today();
        for (i, d) in days.iter().enumerate() {
            let ideal = if days.len() <= 1 {
                0.0
            } else {
                total as f64 * (1.0 - i as f64 / (n - 1.0))
            };
            let remaining = if d.as_str() > today.as_str() {
                None
            } else {
                let closed_by = rows
                    .iter()
                    .filter(|r| {
                        closed(&r.0) && r.2.as_deref().is_some_and(|t| &t[..10] <= d.as_str())
                    })
                    .count() as i64;
                Some(total - closed_by)
            };
            burndown.push(BurnPoint {
                date: d.clone(),
                ideal: (ideal * 100.0).round() / 100.0,
                remaining,
            });
        }
    }
    Progress {
        groups,
        total_items: total,
        completed_items: done,
        total_points,
        completed_points,
        burndown,
    }
}
