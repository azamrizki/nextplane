//! Work items: create, read, filter, update with an activity trail, relations, archive, delete.

use std::collections::HashMap;

use np_core::id::{Uuid, new_id};
use np_core::work_item::{Priority, RelationKind};
use np_core::{Error, Result};
use serde::Serialize;
use sqlx::{FromRow, QueryBuilder, Sqlite, Transaction};

use crate::{Store, db_err, now_ts};

#[derive(Debug, Clone, Serialize)]
pub struct WorkItem {
    pub id: Uuid,
    pub project_id: Uuid,
    pub sequence: i64,
    pub title: String,
    /// Only on single-item reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_json: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_html: Option<String>,
    pub state_id: Uuid,
    pub priority: Priority,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub estimate: Option<String>,
    pub parent_id: Option<Uuid>,
    pub cycle_id: Option<Uuid>,
    pub sort_order: f64,
    pub created_by: Uuid,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub archived_at: Option<String>,
    pub assignee_ids: Vec<Uuid>,
    pub label_ids: Vec<Uuid>,
    pub module_ids: Vec<Uuid>,
    pub sub_item_count: i64,
    pub is_draft: bool,
    /// Submitted through intake and not accepted yet.
    pub in_intake: bool,
}

/// `work_items.is_draft` values (see migration 0014).
pub const DRAFT: i64 = 1;
pub const INTAKE: i64 = 2;

#[derive(FromRow)]
struct Row {
    id: Uuid,
    project_id: Uuid,
    sequence: i64,
    title: String,
    #[sqlx(default)]
    description_json: Option<String>,
    #[sqlx(default)]
    description_html: Option<String>,
    state_id: Uuid,
    priority: String,
    start_date: Option<String>,
    due_date: Option<String>,
    estimate: Option<String>,
    parent_id: Option<Uuid>,
    cycle_id: Option<Uuid>,
    sort_order: f64,
    created_by: Uuid,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
    archived_at: Option<String>,
    #[sqlx(default)]
    sub_item_count: i64,
    is_draft: i64,
}

// Sub-item counts are filled in afterwards for the rows returned (see `fill_counts`); a
// per-row subquery here made SQLite scan the table once per item.
const COLS: &str = "w.id, w.project_id, w.sequence, w.title, w.state_id, w.priority, w.start_date, w.due_date,
    w.estimate, w.parent_id, w.cycle_id, w.sort_order, w.created_by, w.created_at, w.updated_at, w.completed_at, w.archived_at, w.is_draft";

impl Row {
    fn into_item(
        self,
        assignees: Vec<Uuid>,
        labels: Vec<Uuid>,
        modules: Vec<Uuid>,
    ) -> Result<WorkItem> {
        Ok(WorkItem {
            id: self.id,
            project_id: self.project_id,
            sequence: self.sequence,
            title: self.title,
            description_json: self
                .description_json
                .and_then(|j| serde_json::from_str(&j).ok()),
            description_html: self.description_html,
            state_id: self.state_id,
            priority: Priority::parse(&self.priority)?,
            start_date: self.start_date,
            due_date: self.due_date,
            estimate: self.estimate,
            parent_id: self.parent_id,
            cycle_id: self.cycle_id,
            sort_order: self.sort_order,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            completed_at: self.completed_at,
            archived_at: self.archived_at,
            assignee_ids: assignees,
            label_ids: labels,
            module_ids: modules,
            sub_item_count: self.sub_item_count,
            is_draft: self.is_draft == DRAFT,
            in_intake: self.is_draft == INTAKE,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct NewWorkItem {
    pub title: String,
    pub description_json: Option<String>,
    pub description_html: String,
    pub state_id: Option<Uuid>,
    pub priority: Option<Priority>,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub estimate: Option<String>,
    pub parent_id: Option<Uuid>,
    pub cycle_id: Option<Uuid>,
    /// `None` = the project's default assignee, if any.
    pub assignee_ids: Option<Vec<Uuid>>,
    pub label_ids: Vec<Uuid>,
    pub module_ids: Vec<Uuid>,
    pub is_draft: bool,
    /// Created as an intake submission (see [`INTAKE`]).
    pub in_intake: bool,
}

#[derive(Debug, Default, Clone)]
pub struct WorkItemPatch {
    pub title: Option<String>,
    pub description: Option<(Option<String>, String)>,
    pub state_id: Option<Uuid>,
    pub priority: Option<Priority>,
    pub start_date: Option<Option<String>>,
    pub due_date: Option<Option<String>>,
    pub estimate: Option<Option<String>>,
    pub parent_id: Option<Option<Uuid>>,
    pub cycle_id: Option<Option<Uuid>>,
    pub sort_order: Option<f64>,
    pub assignee_ids: Option<Vec<Uuid>>,
    pub label_ids: Option<Vec<Uuid>>,
    pub module_ids: Option<Vec<Uuid>>,
}

impl WorkItemPatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.state_id.is_none()
            && self.priority.is_none()
            && self.start_date.is_none()
            && self.due_date.is_none()
            && self.estimate.is_none()
            && self.parent_id.is_none()
            && self.cycle_id.is_none()
            && self.sort_order.is_none()
            && self.assignee_ids.is_none()
            && self.label_ids.is_none()
            && self.module_ids.is_none()
    }
}

/// List filters; empty vectors mean "any".
#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub state_ids: Vec<Uuid>,
    pub state_groups: Vec<String>,
    pub priorities: Vec<Priority>,
    pub assignee_ids: Vec<Uuid>,
    /// Include items with no assignee.
    pub unassigned: bool,
    pub label_ids: Vec<Uuid>,
    pub created_by: Vec<Uuid>,
    pub parent_id: Option<Uuid>,
    pub cycle_ids: Vec<Uuid>,
    /// Include items in no cycle.
    pub no_cycle: bool,
    pub module_ids: Vec<Uuid>,
    /// Include items in no module.
    pub no_module: bool,
    pub archived: bool,
    pub due_before: Option<String>,
    pub due_after: Option<String>,
    pub search: Option<String>,
    /// Only items created by this user (the guest rule).
    pub only_created_by: Option<Uuid>,
    /// Only items this user follows.
    pub subscribed_by: Option<Uuid>,
    /// Drafts of this user instead of published items.
    pub drafts_of: Option<Uuid>,
    pub start_before: Option<String>,
    pub start_after: Option<String>,
    pub limit: Option<i64>,
    /// Keyset cursor: items after this (sort_order, sequence) in board order.
    pub after: Option<(f64, i64)>,
    /// Newest first instead of board order (workspace views), with its own cursor: items
    /// created before this (created_at, id).
    pub newest_first: bool,
    pub before: Option<(String, Uuid)>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Activity {
    pub id: Uuid,
    pub actor_id: Uuid,
    /// Who acted, for people and service accounts (GitHub, NextPlane automations) alike.
    pub actor_name: String,
    pub actor_is_bot: bool,
    pub verb: String,
    pub field: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Relation {
    pub id: Uuid,
    /// From this item's point of view: blocks, blocked_by, relates_to, duplicate_of, duplicated_by.
    pub kind: String,
    pub item: RelatedItem,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct RelatedItem {
    pub id: Uuid,
    pub project_id: Uuid,
    pub sequence: i64,
    pub title: String,
    pub state_id: Uuid,
    pub identifier: String,
}

async fn activity(
    tx: &mut Transaction<'_, Sqlite>,
    item: Uuid,
    actor: Uuid,
    verb: &str,
    field: Option<&str>,
    old: Option<String>,
    new: Option<String>,
    at: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO work_item_activity (id, work_item_id, actor_id, verb, field, old_value, new_value, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(item)
    .bind(actor)
    .bind(verb)
    .bind(field)
    .bind(old)
    .bind(new)
    .bind(at)
    .execute(&mut **tx)
    .await
    .map_err(db_err)?;
    Ok(())
}

fn hex(u: Uuid) -> String {
    u.simple().to_string()
}

impl Store {
    async fn check_refs(
        tx: &mut Transaction<'_, Sqlite>,
        project: Uuid,
        state: Option<Uuid>,
        assignees: Option<&[Uuid]>,
        labels: Option<&[Uuid]>,
        estimate: Option<&str>,
    ) -> Result<()> {
        if let Some(s) = state {
            let ok: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM states WHERE id = ? AND project_id = ?")
                    .bind(s)
                    .bind(project)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(db_err)?;
            if ok == 0 {
                return Err(Error::bad_request("state is not in this project"));
            }
        }
        if let Some(ids) = assignees {
            for u in ids {
                let ok: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM project_members pm JOIN projects p ON p.id = pm.project_id
                     LEFT JOIN workspace_members wm ON wm.workspace_id = p.workspace_id AND wm.user_id = ? AND wm.deactivated_at IS NULL
                     WHERE (pm.project_id = ? AND pm.user_id = ? AND wm.user_id IS NOT NULL)",
                )
                .bind(u)
                .bind(project)
                .bind(u)
                .fetch_one(&mut **tx)
                .await
                .map_err(db_err)?;
                let ws_admin: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM projects p JOIN workspace_members wm ON wm.workspace_id = p.workspace_id
                     WHERE p.id = ? AND wm.user_id = ? AND wm.deactivated_at IS NULL AND wm.role IN ('owner', 'admin')",
                )
                .bind(project)
                .bind(u)
                .fetch_one(&mut **tx)
                .await
                .map_err(db_err)?;
                if ok == 0 && ws_admin == 0 {
                    return Err(Error::bad_request("assignees must be project members"));
                }
            }
        }
        if let Some(ids) = labels {
            for l in ids {
                let ok: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM labels WHERE id = ? AND project_id = ?",
                )
                .bind(l)
                .bind(project)
                .fetch_one(&mut **tx)
                .await
                .map_err(db_err)?;
                if ok == 0 {
                    return Err(Error::bad_request("label is not in this project"));
                }
            }
        }
        if let Some(e) = estimate {
            let vals: String =
                sqlx::query_scalar("SELECT estimate_values FROM projects WHERE id = ?")
                    .bind(project)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(db_err)?;
            let vals: Vec<String> = serde_json::from_str(&vals).unwrap_or_default();
            if !vals.iter().any(|v| v == e) {
                return Err(Error::bad_request(
                    "estimate is not on this project's scale",
                ));
            }
        }
        Ok(())
    }

    /// Refuses a parent outside the project, a deleted parent, or one that would make a loop.
    async fn check_parent(
        tx: &mut Transaction<'_, Sqlite>,
        project: Uuid,
        item: Option<Uuid>,
        parent: Uuid,
    ) -> Result<()> {
        if Some(parent) == item {
            return Err(Error::bad_request("a work item cannot be its own parent"));
        }
        let mut cur = Some(parent);
        let mut depth = 0;
        while let Some(c) = cur {
            let row: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
                "SELECT project_id, parent_id FROM work_items WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(c)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db_err)?;
            let Some((p, next)) = row else {
                return Err(Error::bad_request("parent work item not found"));
            };
            if depth == 0 && p != project {
                return Err(Error::bad_request("the parent must be in the same project"));
            }
            if Some(c) == item && depth > 0 {
                return Err(Error::bad_request("this would make a loop of parents"));
            }
            if next == item && item.is_some() {
                return Err(Error::bad_request("this would make a loop of parents"));
            }
            cur = next;
            depth += 1;
            if depth > 100 {
                return Err(Error::bad_request("parent chain is too deep"));
            }
        }
        Ok(())
    }

    /// Live, unarchived modules of this project.
    async fn check_modules(
        tx: &mut Transaction<'_, Sqlite>,
        project: Uuid,
        ids: &[Uuid],
    ) -> Result<()> {
        for m in ids {
            let ok: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM modules WHERE id = ? AND project_id = ? AND deleted_at IS NULL AND archived_at IS NULL",
            )
            .bind(m)
            .bind(project)
            .fetch_one(&mut **tx)
            .await
            .map_err(db_err)?;
            if ok == 0 {
                return Err(Error::bad_request(
                    "module is not in this project or is archived",
                ));
            }
        }
        Ok(())
    }

    /// A cycle of this project that is live, not archived and not completed.
    async fn check_cycle(
        tx: &mut Transaction<'_, Sqlite>,
        project: Uuid,
        cycle: Uuid,
    ) -> Result<()> {
        let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT start_date, end_date, archived_at FROM cycles WHERE id = ? AND project_id = ? AND deleted_at IS NULL",
        )
        .bind(cycle)
        .bind(project)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_err)?;
        let Some((s, e, archived)) = row else {
            return Err(Error::bad_request("cycle is not in this project"));
        };
        if archived.is_some() {
            return Err(Error::bad_request("this cycle is archived"));
        }
        let today = &crate::now_ts()[..10];
        if np_core::cycle::status(s.as_deref(), e.as_deref(), today)
            == np_core::cycle::CycleStatus::Completed
        {
            return Err(Error::bad_request(
                "work items cannot be added to a completed cycle",
            ));
        }
        Ok(())
    }

    async fn state_closes(tx: &mut Transaction<'_, Sqlite>, state: Uuid) -> Result<bool> {
        let g: String = sqlx::query_scalar("SELECT grp FROM states WHERE id = ?")
            .bind(state)
            .fetch_one(&mut **tx)
            .await
            .map_err(db_err)?;
        Ok(g == "completed" || g == "cancelled")
    }

    pub async fn create_work_item(
        &self,
        project: Uuid,
        n: &NewWorkItem,
        actor: Uuid,
    ) -> Result<WorkItem> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let (seq, default_assignee): (i64, Option<Uuid>) = sqlx::query_as(
            "UPDATE projects SET next_sequence = next_sequence + 1 WHERE id = ? RETURNING next_sequence - 1, default_assignee_id",
        )
        .bind(project)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        let state = match n.state_id {
            Some(s) => s,
            None => sqlx::query_scalar("SELECT id FROM states WHERE project_id = ? ORDER BY is_default DESC, sort_order LIMIT 1")
                .bind(project)
                .fetch_one(&mut *tx)
                .await
                .map_err(db_err)?,
        };
        let assignees = n
            .assignee_ids
            .clone()
            .unwrap_or_else(|| default_assignee.into_iter().collect());
        Self::check_refs(
            &mut tx,
            project,
            Some(state),
            Some(&assignees),
            Some(&n.label_ids),
            n.estimate.as_deref(),
        )
        .await?;
        if let Some(p) = n.parent_id {
            Self::check_parent(&mut tx, project, None, p).await?;
        }
        if let Some(c) = n.cycle_id {
            Self::check_cycle(&mut tx, project, c).await?;
        }
        Self::check_modules(&mut tx, project, &n.module_ids).await?;
        let id = new_id();
        let now = now_ts();
        let min: Option<f64> = sqlx::query_scalar(
            "SELECT MIN(sort_order) FROM work_items WHERE project_id = ? AND state_id = ?",
        )
        .bind(project)
        .bind(state)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        let completed = if Self::state_closes(&mut tx, state).await? {
            Some(now.clone())
        } else {
            None
        };
        sqlx::query(
            "INSERT INTO work_items (id, project_id, sequence, title, description_json, description_html, state_id, priority,
               start_date, due_date, estimate, parent_id, sort_order, created_by, created_at, updated_at, completed_at, cycle_id, is_draft)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(project)
        .bind(seq)
        .bind(&n.title)
        .bind(&n.description_json)
        .bind(&n.description_html)
        .bind(state)
        .bind(n.priority.unwrap_or(Priority::None).as_str())
        .bind(&n.start_date)
        .bind(&n.due_date)
        .bind(&n.estimate)
        .bind(n.parent_id)
        .bind(min.map(|m| m - 1000.0).unwrap_or(65535.0))
        .bind(actor)
        .bind(&now)
        .bind(&now)
        .bind(completed)
        .bind(n.cycle_id)
        .bind(if n.in_intake { INTAKE } else if n.is_draft { DRAFT } else { 0 })
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        for u in &assignees {
            sqlx::query(
                "INSERT OR IGNORE INTO work_item_assignees (work_item_id, user_id) VALUES (?, ?)",
            )
            .bind(id)
            .bind(u)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }
        for l in &n.label_ids {
            sqlx::query(
                "INSERT OR IGNORE INTO work_item_labels (work_item_id, label_id) VALUES (?, ?)",
            )
            .bind(id)
            .bind(l)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }
        for m in &n.module_ids {
            sqlx::query(
                "INSERT OR IGNORE INTO work_item_modules (work_item_id, module_id) VALUES (?, ?)",
            )
            .bind(id)
            .bind(m)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }
        activity(&mut tx, id, actor, "created", None, None, None, &now).await?;
        for u in std::iter::once(&actor).chain(assignees.iter()) {
            sqlx::query("INSERT OR IGNORE INTO subscriptions (work_item_id, user_id, created_at) VALUES (?, ?, ?)")
                .bind(id)
                .bind(u)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        self.index_work_item(id).await?;
        if !n.is_draft && !n.in_intake {
            self.notify(
                id,
                actor,
                &assignees,
                &crate::notifications::Note {
                    kind: "assigned",
                    field: None,
                    old: None,
                    new: None,
                    comment: None,
                },
            )
            .await?;
        }
        self.work_item(project, id)
            .await?
            .ok_or_else(|| Error::not_found("work item not found"))
    }

    /// One live work item with its description.
    pub async fn work_item(&self, project: Uuid, id: Uuid) -> Result<Option<WorkItem>> {
        let row: Option<Row> = sqlx::query_as(&format!(
            "SELECT {COLS}, w.description_json, w.description_html FROM work_items w
             WHERE w.id = ? AND w.project_id = ? AND w.deleted_at IS NULL"
        ))
        .bind(id)
        .bind(project)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        let Some(mut row) = row else { return Ok(None) };
        row.sub_item_count = self.sub_counts(&[id]).await?.get(&id).copied().unwrap_or(0);
        let (mut a, mut l, mut m) = self.links(&[id]).await?;
        let item = row.into_item(
            a.remove(&id).unwrap_or_default(),
            l.remove(&id).unwrap_or_default(),
            m.remove(&id).unwrap_or_default(),
        )?;
        Ok(Some(item))
    }

    /// The project a work item belongs to, by id alone (for links).
    pub async fn work_item_project(&self, id: Uuid) -> Result<Option<Uuid>> {
        sqlx::query_scalar("SELECT project_id FROM work_items WHERE id = ? AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)
    }

    /// Finds a work item by project identifier and sequence (`WEB-12`) inside a workspace.
    pub async fn work_item_by_key(
        &self,
        ws: Uuid,
        identifier: &str,
        seq: i64,
    ) -> Result<Option<(Uuid, Uuid)>> {
        sqlx::query_as(
            "SELECT w.project_id, w.id FROM work_items w JOIN projects p ON p.id = w.project_id
             WHERE p.workspace_id = ? AND p.identifier = ? AND w.sequence = ? AND w.deleted_at IS NULL AND w.is_draft = 0 AND p.deleted_at IS NULL",
        )
        .bind(ws)
        .bind(identifier)
        .bind(seq)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    /// Live sub-item counts for the given parents.
    async fn sub_counts(&self, ids: &[Uuid]) -> Result<HashMap<Uuid, i64>> {
        let mut out = HashMap::new();
        for chunk in ids.chunks(500) {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "SELECT parent_id, COUNT(*) FROM work_items INDEXED BY work_items_parent WHERE deleted_at IS NULL AND is_draft = 0 AND parent_id IN (",
            );
            let mut sep = qb.separated(", ");
            for id in chunk {
                sep.push_bind(*id);
            }
            qb.push(") GROUP BY parent_id");
            for (p, n) in qb
                .build_query_as::<(Uuid, i64)>()
                .fetch_all(self.pool())
                .await
                .map_err(db_err)?
            {
                out.insert(p, n);
            }
        }
        Ok(out)
    }

    #[allow(clippy::type_complexity)]
    async fn links(
        &self,
        ids: &[Uuid],
    ) -> Result<(
        HashMap<Uuid, Vec<Uuid>>,
        HashMap<Uuid, Vec<Uuid>>,
        HashMap<Uuid, Vec<Uuid>>,
    )> {
        let mut a: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        let mut l: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        let mut m: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for chunk in ids.chunks(500) {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "SELECT work_item_id, user_id FROM work_item_assignees WHERE work_item_id IN (",
            );
            let mut sep = qb.separated(", ");
            for id in chunk {
                sep.push_bind(*id);
            }
            qb.push(")");
            for (w, u) in qb
                .build_query_as::<(Uuid, Uuid)>()
                .fetch_all(self.pool())
                .await
                .map_err(db_err)?
            {
                a.entry(w).or_default().push(u);
            }
            let mut qb = QueryBuilder::<Sqlite>::new(
                "SELECT work_item_id, label_id FROM work_item_labels WHERE work_item_id IN (",
            );
            let mut sep = qb.separated(", ");
            for id in chunk {
                sep.push_bind(*id);
            }
            qb.push(")");
            for (w, x) in qb
                .build_query_as::<(Uuid, Uuid)>()
                .fetch_all(self.pool())
                .await
                .map_err(db_err)?
            {
                l.entry(w).or_default().push(x);
            }
            let mut qb = QueryBuilder::<Sqlite>::new(
                "SELECT x.work_item_id, x.module_id FROM work_item_modules x JOIN modules mo ON mo.id = x.module_id AND mo.deleted_at IS NULL WHERE x.work_item_id IN (",
            );
            let mut sep = qb.separated(", ");
            for id in chunk {
                sep.push_bind(*id);
            }
            qb.push(")");
            for (w, x) in qb
                .build_query_as::<(Uuid, Uuid)>()
                .fetch_all(self.pool())
                .await
                .map_err(db_err)?
            {
                m.entry(w).or_default().push(x);
            }
        }
        Ok((a, l, m))
    }

    /// Work items of a project matching `f`, without descriptions, in board order.
    pub async fn work_items(&self, project: Uuid, f: &Filter) -> Result<Vec<WorkItem>> {
        self.work_items_in(&[project], f).await
    }

    /// Work items of several projects matching `f`: in board order, or newest first with
    /// `f.newest_first` (workspace views).
    pub async fn work_items_in(&self, projects: &[Uuid], f: &Filter) -> Result<Vec<WorkItem>> {
        if projects.is_empty() {
            return Ok(Vec::new());
        }
        let mut qb = QueryBuilder::<Sqlite>::new(format!(
            "SELECT {COLS} FROM work_items w WHERE w.project_id IN ("
        ));
        {
            let mut sep = qb.separated(", ");
            for p in projects {
                sep.push_bind(*p);
            }
        }
        qb.push(")");
        match f.drafts_of {
            Some(u) => {
                qb.push(" AND w.is_draft = 1 AND w.created_by = ")
                    .push_bind(u);
            }
            None => {
                qb.push(" AND w.is_draft = 0");
            }
        }
        qb.push(" AND w.deleted_at IS NULL AND w.archived_at IS ");
        qb.push(if f.archived { "NOT NULL" } else { "NULL" });
        fn any_of<
            T: Copy + for<'q> sqlx::Encode<'q, Sqlite> + sqlx::Type<Sqlite> + Send + 'static,
        >(
            qb: &mut QueryBuilder<'_, Sqlite>,
            col: &str,
            vals: &[T],
        ) {
            qb.push(format!(" AND {col} IN ("));
            let mut sep = qb.separated(", ");
            for v in vals {
                sep.push_bind(*v);
            }
            qb.push(")");
        }
        if !f.state_ids.is_empty() {
            any_of(&mut qb, "w.state_id", &f.state_ids);
        }
        if !f.state_groups.is_empty() {
            qb.push(" AND w.state_id IN (SELECT id FROM states WHERE grp IN (");
            let mut sep = qb.separated(", ");
            for g in &f.state_groups {
                sep.push_bind(g.clone());
            }
            qb.push("))");
        }
        if !f.priorities.is_empty() {
            qb.push(" AND w.priority IN (");
            let mut sep = qb.separated(", ");
            for p in &f.priorities {
                sep.push_bind(p.as_str());
            }
            qb.push(")");
        }
        if !f.assignee_ids.is_empty() || f.unassigned {
            qb.push(" AND (");
            if !f.assignee_ids.is_empty() {
                qb.push("w.id IN (SELECT work_item_id FROM work_item_assignees WHERE user_id IN (");
                let mut sep = qb.separated(", ");
                for u in &f.assignee_ids {
                    sep.push_bind(*u);
                }
                qb.push("))");
            }
            if f.unassigned {
                if !f.assignee_ids.is_empty() {
                    qb.push(" OR ");
                }
                qb.push(
                    "NOT EXISTS (SELECT 1 FROM work_item_assignees x WHERE x.work_item_id = w.id)",
                );
            }
            qb.push(")");
        }
        if !f.label_ids.is_empty() {
            qb.push(" AND w.id IN (SELECT work_item_id FROM work_item_labels WHERE label_id IN (");
            let mut sep = qb.separated(", ");
            for l in &f.label_ids {
                sep.push_bind(*l);
            }
            qb.push("))");
        }
        if !f.created_by.is_empty() {
            any_of(&mut qb, "w.created_by", &f.created_by);
        }
        if let Some(u) = f.only_created_by {
            qb.push(" AND w.created_by = ").push_bind(u);
        }
        if let Some(u) = f.subscribed_by {
            qb.push(" AND w.id IN (SELECT work_item_id FROM subscriptions WHERE user_id = ")
                .push_bind(u)
                .push(")");
        }
        if let Some(p) = f.parent_id {
            qb.push(" AND w.parent_id = ").push_bind(p);
        }
        if !f.module_ids.is_empty() || f.no_module {
            qb.push(" AND (");
            if !f.module_ids.is_empty() {
                qb.push("w.id IN (SELECT work_item_id FROM work_item_modules WHERE module_id IN (");
                let mut sep = qb.separated(", ");
                for m in &f.module_ids {
                    sep.push_bind(*m);
                }
                qb.push("))");
            }
            if f.no_module {
                if !f.module_ids.is_empty() {
                    qb.push(" OR ");
                }
                qb.push("NOT EXISTS (SELECT 1 FROM work_item_modules x JOIN modules mo ON mo.id = x.module_id AND mo.deleted_at IS NULL WHERE x.work_item_id = w.id)");
            }
            qb.push(")");
        }
        if !f.cycle_ids.is_empty() || f.no_cycle {
            qb.push(" AND (");
            if !f.cycle_ids.is_empty() {
                qb.push("w.cycle_id IN (");
                let mut sep = qb.separated(", ");
                for c in &f.cycle_ids {
                    sep.push_bind(*c);
                }
                qb.push(")");
            }
            if f.no_cycle {
                if !f.cycle_ids.is_empty() {
                    qb.push(" OR ");
                }
                qb.push("w.cycle_id IS NULL");
            }
            qb.push(")");
        }
        if let Some(d) = &f.due_before {
            qb.push(" AND w.due_date <= ").push_bind(d.clone());
        }
        if let Some(d) = &f.due_after {
            qb.push(" AND w.due_date >= ").push_bind(d.clone());
        }
        if let Some(q) = f.search.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            let like = format!(
                "%{}%",
                q.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            qb.push(" AND (w.title LIKE ")
                .push_bind(like)
                .push(" ESCAPE '\\'");
            if let Some(n) = q.rsplit('-').next().and_then(|n| n.parse::<i64>().ok()) {
                qb.push(" OR w.sequence = ").push_bind(n);
            }
            qb.push(")");
        }
        if let Some(d) = &f.start_before {
            qb.push(" AND w.start_date <= ").push_bind(d.clone());
        }
        if let Some(d) = &f.start_after {
            qb.push(" AND w.start_date >= ").push_bind(d.clone());
        }
        if f.newest_first {
            if let Some((at, id)) = &f.before {
                qb.push(" AND (w.created_at < ")
                    .push_bind(at.clone())
                    .push(" OR (w.created_at = ")
                    .push_bind(at.clone())
                    .push(" AND w.id < ")
                    .push_bind(*id)
                    .push("))");
            }
            qb.push(" ORDER BY w.created_at DESC, w.id DESC");
        } else {
            if let Some((o, seq)) = f.after {
                qb.push(" AND (w.sort_order > ")
                    .push_bind(o)
                    .push(" OR (w.sort_order = ")
                    .push_bind(o)
                    .push(" AND w.sequence < ")
                    .push_bind(seq)
                    .push("))");
            }
            qb.push(" ORDER BY w.sort_order, w.sequence DESC");
        }
        if let Some(l) = f.limit {
            qb.push(" LIMIT ").push_bind(l);
        }
        let mut rows: Vec<Row> = qb
            .build_query_as()
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
        let counts = self.sub_counts(&ids).await?;
        for r in &mut rows {
            r.sub_item_count = counts.get(&r.id).copied().unwrap_or(0);
        }
        let (mut a, mut l, mut m) = self.links(&ids).await?;
        rows.into_iter()
            .map(|r| {
                let id = r.id;
                r.into_item(
                    a.remove(&id).unwrap_or_default(),
                    l.remove(&id).unwrap_or_default(),
                    m.remove(&id).unwrap_or_default(),
                )
            })
            .collect()
    }

    /// Applies a patch and writes one activity row per changed field.
    pub async fn update_work_item(
        &self,
        project: Uuid,
        id: Uuid,
        p: &WorkItemPatch,
        actor: Uuid,
    ) -> Result<WorkItem> {
        let cur = self
            .work_item(project, id)
            .await?
            .ok_or_else(|| Error::not_found("work item not found"))?;
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let now = now_ts();
        Self::check_refs(
            &mut tx,
            project,
            p.state_id,
            p.assignee_ids.as_deref(),
            p.label_ids.as_deref(),
            p.estimate.as_ref().and_then(|e| e.as_deref()),
        )
        .await?;
        if let Some(Some(parent)) = p.parent_id {
            Self::check_parent(&mut tx, project, Some(id), parent).await?;
        }
        if let Some(Some(c)) = p.cycle_id
            && Some(c) != cur.cycle_id
        {
            Self::check_cycle(&mut tx, project, c).await?;
        }
        if let Some(ms) = &p.module_ids {
            let added: Vec<Uuid> = ms
                .iter()
                .filter(|m| !cur.module_ids.contains(m))
                .copied()
                .collect();
            Self::check_modules(&mut tx, project, &added).await?;
        }
        let start = p.start_date.clone().unwrap_or(cur.start_date.clone());
        let due = p.due_date.clone().unwrap_or(cur.due_date.clone());
        np_core::work_item::check_date_order(start.as_deref(), due.as_deref())?;

        macro_rules! changed {
            ($field:literal, $old:expr, $new:expr) => {
                if $old != $new {
                    activity(
                        &mut tx,
                        id,
                        actor,
                        "updated",
                        Some($field),
                        $old.map(|v| v.to_string()),
                        $new.map(|v| v.to_string()),
                        &now,
                    )
                    .await?;
                }
            };
        }
        if let Some(t) = &p.title {
            changed!("title", Some(cur.title.as_str()), Some(t.as_str()));
        }
        if p.description.is_some() {
            activity(
                &mut tx,
                id,
                actor,
                "updated",
                Some("description"),
                None,
                None,
                &now,
            )
            .await?;
        }
        let mut completed = cur.completed_at.clone();
        if let Some(s) = p.state_id
            && s != cur.state_id
        {
            changed!("state", Some(hex(cur.state_id)), Some(hex(s)));
            completed = if Self::state_closes(&mut tx, s).await? {
                Some(now.clone())
            } else {
                None
            };
        }
        if let Some(pr) = p.priority {
            changed!("priority", Some(cur.priority.as_str()), Some(pr.as_str()));
        }
        if let Some(d) = &p.start_date {
            changed!("start_date", cur.start_date.as_deref(), d.as_deref());
        }
        if let Some(d) = &p.due_date {
            changed!("due_date", cur.due_date.as_deref(), d.as_deref());
        }
        if let Some(e) = &p.estimate {
            changed!("estimate", cur.estimate.as_deref(), e.as_deref());
        }
        if let Some(par) = &p.parent_id {
            changed!("parent", cur.parent_id.map(hex), par.map(hex));
        }
        if let Some(c) = &p.cycle_id {
            changed!("cycle", cur.cycle_id.map(hex), c.map(hex));
        }
        sqlx::query(
            "UPDATE work_items SET
               title = COALESCE(?, title),
               description_json = CASE WHEN ? THEN ? ELSE description_json END,
               description_html = CASE WHEN ? THEN ? ELSE description_html END,
               state_id = COALESCE(?, state_id),
               priority = COALESCE(?, priority),
               start_date = CASE WHEN ? THEN ? ELSE start_date END,
               due_date = CASE WHEN ? THEN ? ELSE due_date END,
               estimate = CASE WHEN ? THEN ? ELSE estimate END,
               parent_id = CASE WHEN ? THEN ? ELSE parent_id END,
               cycle_id = CASE WHEN ? THEN ? ELSE cycle_id END,
               sort_order = COALESCE(?, sort_order),
               completed_at = ?,
               updated_at = ?
             WHERE id = ? AND project_id = ?",
        )
        .bind(&p.title)
        .bind(p.description.is_some())
        .bind(p.description.as_ref().and_then(|d| d.0.clone()))
        .bind(p.description.is_some())
        .bind(
            p.description
                .as_ref()
                .map(|d| d.1.clone())
                .unwrap_or_default(),
        )
        .bind(p.state_id)
        .bind(p.priority.map(Priority::as_str))
        .bind(p.start_date.is_some())
        .bind(p.start_date.clone().flatten())
        .bind(p.due_date.is_some())
        .bind(p.due_date.clone().flatten())
        .bind(p.estimate.is_some())
        .bind(p.estimate.clone().flatten())
        .bind(p.parent_id.is_some())
        .bind(p.parent_id.flatten())
        .bind(p.cycle_id.is_some())
        .bind(p.cycle_id.flatten())
        .bind(p.sort_order)
        .bind(completed)
        .bind(&now)
        .bind(id)
        .bind(project)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        if let Some(new) = &p.assignee_ids {
            for u in new.iter().filter(|u| !cur.assignee_ids.contains(u)) {
                sqlx::query("INSERT OR IGNORE INTO work_item_assignees (work_item_id, user_id) VALUES (?, ?)")
                    .bind(id)
                    .bind(u)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
                activity(
                    &mut tx,
                    id,
                    actor,
                    "added",
                    Some("assignee"),
                    None,
                    Some(hex(*u)),
                    &now,
                )
                .await?;
                sqlx::query("INSERT OR IGNORE INTO subscriptions (work_item_id, user_id, created_at) VALUES (?, ?, ?)")
                    .bind(id)
                    .bind(u)
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
            }
            for u in cur.assignee_ids.iter().filter(|u| !new.contains(u)) {
                sqlx::query(
                    "DELETE FROM work_item_assignees WHERE work_item_id = ? AND user_id = ?",
                )
                .bind(id)
                .bind(u)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
                activity(
                    &mut tx,
                    id,
                    actor,
                    "removed",
                    Some("assignee"),
                    Some(hex(*u)),
                    None,
                    &now,
                )
                .await?;
            }
        }
        if let Some(new) = &p.label_ids {
            for l in new.iter().filter(|l| !cur.label_ids.contains(l)) {
                sqlx::query(
                    "INSERT OR IGNORE INTO work_item_labels (work_item_id, label_id) VALUES (?, ?)",
                )
                .bind(id)
                .bind(l)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
                activity(
                    &mut tx,
                    id,
                    actor,
                    "added",
                    Some("label"),
                    None,
                    Some(hex(*l)),
                    &now,
                )
                .await?;
            }
            for l in cur.label_ids.iter().filter(|l| !new.contains(l)) {
                sqlx::query("DELETE FROM work_item_labels WHERE work_item_id = ? AND label_id = ?")
                    .bind(id)
                    .bind(l)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
                activity(
                    &mut tx,
                    id,
                    actor,
                    "removed",
                    Some("label"),
                    Some(hex(*l)),
                    None,
                    &now,
                )
                .await?;
            }
        }
        if let Some(new) = &p.module_ids {
            for m in new.iter().filter(|m| !cur.module_ids.contains(m)) {
                sqlx::query("INSERT OR IGNORE INTO work_item_modules (work_item_id, module_id) VALUES (?, ?)")
                    .bind(id)
                    .bind(m)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
                activity(
                    &mut tx,
                    id,
                    actor,
                    "added",
                    Some("module"),
                    None,
                    Some(hex(*m)),
                    &now,
                )
                .await?;
            }
            for m in cur.module_ids.iter().filter(|m| !new.contains(m)) {
                sqlx::query(
                    "DELETE FROM work_item_modules WHERE work_item_id = ? AND module_id = ?",
                )
                .bind(id)
                .bind(m)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
                activity(
                    &mut tx,
                    id,
                    actor,
                    "removed",
                    Some("module"),
                    Some(hex(*m)),
                    None,
                    &now,
                )
                .await?;
            }
        }
        tx.commit().await.map_err(db_err)?;
        if p.title.is_some() || p.description.is_some() {
            self.index_work_item(id).await?;
        }
        self.notify_from_activity(id, actor, &now).await?;
        self.work_item(project, id)
            .await?
            .ok_or_else(|| Error::not_found("work item not found"))
    }

    pub async fn set_work_item_archived(
        &self,
        project: Uuid,
        id: Uuid,
        on: bool,
        actor: Uuid,
    ) -> Result<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let now = now_ts();
        let r = sqlx::query(
            "UPDATE work_items SET archived_at = CASE WHEN ? THEN ? ELSE NULL END, updated_at = ?
             WHERE id = ? AND project_id = ? AND deleted_at IS NULL",
        )
        .bind(on)
        .bind(&now)
        .bind(&now)
        .bind(id)
        .bind(project)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("work item not found"));
        }
        activity(
            &mut tx,
            id,
            actor,
            if on { "archived" } else { "restored" },
            None,
            None,
            None,
            &now,
        )
        .await?;
        tx.commit().await.map_err(db_err)?;
        self.notify_from_activity(id, actor, &now).await
    }

    /// Deletes a work item (kept with `deleted_at`, never shown again). Its sub-items lose
    /// their parent.
    pub async fn delete_work_item(&self, project: Uuid, id: Uuid) -> Result<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let now = now_ts();
        let r = sqlx::query("UPDATE work_items SET deleted_at = ?, updated_at = ? WHERE id = ? AND project_id = ? AND deleted_at IS NULL")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .bind(project)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("work item not found"));
        }
        sqlx::query("UPDATE work_items SET parent_id = NULL WHERE parent_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("DELETE FROM work_item_relations WHERE from_id = ? OR to_id = ?")
            .bind(id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        self.index_work_item(id).await
    }

    pub async fn work_item_activity(&self, id: Uuid) -> Result<Vec<Activity>> {
        sqlx::query_as(
            "SELECT a.id, a.actor_id, COALESCE(NULLIF(u.name, ''), u.email, '') AS actor_name,
               COALESCE(u.is_bot, 0) AS actor_is_bot, a.verb, a.field, a.old_value, a.new_value, a.created_at
             FROM work_item_activity a LEFT JOIN users u ON u.id = a.actor_id
             WHERE a.work_item_id = ? ORDER BY a.created_at, a.id",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    // ---- relations ----------------------------------------------------------------------

    /// Adds a relation to another live item in the same workspace.
    pub async fn add_relation(
        &self,
        ws: Uuid,
        item: Uuid,
        other: Uuid,
        kind: RelationKind,
        actor: Uuid,
    ) -> Result<()> {
        if item == other {
            return Err(Error::bad_request("a work item cannot relate to itself"));
        }
        let same_ws: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM work_items w JOIN projects p ON p.id = w.project_id
             WHERE w.id = ? AND p.workspace_id = ? AND w.deleted_at IS NULL AND p.deleted_at IS NULL",
        )
        .bind(other)
        .bind(ws)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        if same_ws == 0 {
            return Err(Error::bad_request(
                "related work item not found in this workspace",
            ));
        }
        let (k, from, to) = kind.stored(item, other);
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let now = now_ts();
        sqlx::query("INSERT INTO work_item_relations (id, from_id, to_id, kind, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(new_id())
            .bind(from)
            .bind(to)
            .bind(k)
            .bind(actor)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|e| match db_err(e) {
                e if e.kind == np_core::ErrorKind::Conflict => Error::conflict("these work items are already related this way"),
                e => e,
            })?;
        activity(
            &mut tx,
            item,
            actor,
            "added",
            Some("relation"),
            None,
            Some(format!("{}:{}", rel_name(kind), hex(other))),
            &now,
        )
        .await?;
        tx.commit().await.map_err(db_err)
    }

    pub async fn remove_relation(&self, item: Uuid, relation: Uuid, actor: Uuid) -> Result<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let r = sqlx::query(
            "DELETE FROM work_item_relations WHERE id = ? AND (from_id = ? OR to_id = ?)",
        )
        .bind(relation)
        .bind(item)
        .bind(item)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("relation not found"));
        }
        activity(
            &mut tx,
            item,
            actor,
            "removed",
            Some("relation"),
            None,
            None,
            &now_ts(),
        )
        .await?;
        tx.commit().await.map_err(db_err)
    }

    /// Relations of an item, from its point of view, with the other item's key and title.
    pub async fn relations(&self, item: Uuid) -> Result<Vec<Relation>> {
        #[derive(FromRow)]
        struct RelRow {
            rid: Uuid,
            kind: String,
            outgoing: bool,
            #[sqlx(flatten)]
            item: RelatedItem,
        }
        let rows: Vec<RelRow> = sqlx::query_as(
            "SELECT r.id AS rid, r.kind, (r.from_id = ?) AS outgoing, w.id, w.project_id, w.sequence, w.title, w.state_id, p.identifier
             FROM work_item_relations r
             JOIN work_items w ON w.id = CASE WHEN r.from_id = ? THEN r.to_id ELSE r.from_id END
             JOIN projects p ON p.id = w.project_id
             WHERE (r.from_id = ? OR r.to_id = ?) AND w.deleted_at IS NULL AND p.deleted_at IS NULL
             ORDER BY r.created_at",
        )
        .bind(item)
        .bind(item)
        .bind(item)
        .bind(item)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let kind = match (r.kind.as_str(), r.outgoing) {
                    ("blocks", true) => "blocks",
                    ("blocks", false) => "blocked_by",
                    ("duplicate_of", true) => "duplicate_of",
                    ("duplicate_of", false) => "duplicated_by",
                    _ => "relates_to",
                };
                Relation {
                    id: r.rid,
                    kind: kind.into(),
                    item: r.item,
                }
            })
            .collect())
    }
}

fn rel_name(k: RelationKind) -> &'static str {
    match k {
        RelationKind::Blocks => "blocks",
        RelationKind::BlockedBy => "blocked_by",
        RelationKind::RelatesTo => "relates_to",
        RelationKind::DuplicateOf => "duplicate_of",
    }
}

impl Store {
    /// Whether someone other than `me` changed the title or description after `since`.
    pub async fn text_changed_by_others(&self, item: Uuid, since: &str, me: Uuid) -> Result<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM work_item_activity
             WHERE work_item_id = ? AND field IN ('title', 'description') AND created_at > ? AND actor_id <> ?",
        )
        .bind(item)
        .bind(since)
        .bind(me)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(n > 0)
    }
}
