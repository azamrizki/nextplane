//! Intake: submissions waiting for triage (SPRINT S3.3, PERMISSIONS §2). A submission is a
//! work item hidden from the project (`is_draft = 2`) plus a row in `intake_items`.

use np_core::id::{Uuid, new_id};
use np_core::work_item::Priority;
use np_core::{Error, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::notifications::Note;
use crate::work_items::{INTAKE, NewWorkItem, WorkItem};
use crate::{Store, db_err, now_ts};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct IntakeEntry {
    pub work_item_id: Uuid,
    pub sequence: i64,
    pub title: String,
    pub priority: String,
    pub created_by: Uuid,
    pub created_at: String,
    pub status: String,
    pub snoozed_until: Option<String>,
    pub duplicate_of: Option<Uuid>,
    pub duplicate_sequence: Option<i64>,
    pub reason: Option<String>,
    pub source: String,
    pub submitter_name: Option<String>,
    pub submitter_email: Option<String>,
    pub triaged_by: Option<Uuid>,
    pub triaged_at: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_html: Option<String>,
}

/// Which part of the queue to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntakeView {
    /// Pending, and snoozed ones whose time has come.
    Open,
    Snoozed,
    /// Accepted, declined, duplicate.
    Closed,
    All,
}

impl IntakeView {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "open" => Self::Open,
            "snoozed" => Self::Snoozed,
            "closed" => Self::Closed,
            "all" => Self::All,
            _ => return None,
        })
    }
}

/// Where a submission comes from.
pub struct Submission<'a> {
    pub title: &'a str,
    pub description_html: &'a str,
    pub priority: Option<Priority>,
    pub source: &'a str,
    pub submitter_name: Option<&'a str>,
    pub submitter_email: Option<&'a str>,
}

const ENTRY_COLS: &str = "i.work_item_id, w.sequence, w.title, w.priority, w.created_by, i.created_at, i.status, i.snoozed_until,
    i.duplicate_of, d.sequence AS duplicate_sequence, i.reason, i.source, i.submitter_name, i.submitter_email, i.triaged_by, i.triaged_at";
const ENTRY_FROM: &str =
    "FROM intake_items i JOIN work_items w ON w.id = i.work_item_id AND w.deleted_at IS NULL
    LEFT JOIN work_items d ON d.id = i.duplicate_of";

impl Store {
    pub async fn submit_intake(
        &self,
        project: Uuid,
        by: Uuid,
        s: &Submission<'_>,
    ) -> Result<WorkItem> {
        let n = NewWorkItem {
            title: s.title.to_string(),
            description_html: s.description_html.to_string(),
            priority: s.priority,
            assignee_ids: Some(vec![]),
            in_intake: true,
            ..Default::default()
        };
        let item = self.create_work_item(project, &n, by).await?;
        sqlx::query(
            "INSERT INTO intake_items (work_item_id, project_id, source, submitter_name, submitter_email, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(item.id)
        .bind(project)
        .bind(s.source)
        .bind(s.submitter_name)
        .bind(s.submitter_email)
        .bind(&item.created_at)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        // Tell the people who triage.
        let admins: Vec<Uuid> = sqlx::query_scalar(
            "SELECT pm.user_id FROM project_members pm WHERE pm.project_id = ?1 AND pm.role = 'admin'
             UNION SELECT wm.user_id FROM workspace_members wm JOIN projects p ON p.workspace_id = wm.workspace_id
               WHERE p.id = ?1 AND wm.role IN ('owner', 'admin') AND wm.deactivated_at IS NULL",
        )
        .bind(project)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        self.notify(
            item.id,
            by,
            &admins,
            &Note {
                kind: "updated",
                field: Some("intake"),
                old: None,
                new: Some("pending"),
                comment: None,
            },
        )
        .await?;
        Ok(item)
    }

    /// The queue of a project; `only_by` limits it to one submitter (guests see their own).
    pub async fn intake(
        &self,
        project: Uuid,
        view: IntakeView,
        only_by: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<IntakeEntry>> {
        let now = now_ts();
        let cond = match view {
            IntakeView::Open => {
                "(i.status = 'pending' OR (i.status = 'snoozed' AND i.snoozed_until <= ?2))"
            }
            IntakeView::Snoozed => "(i.status = 'snoozed' AND i.snoozed_until > ?2)",
            IntakeView::Closed => {
                "(i.status IN ('accepted', 'declined', 'duplicate') AND ?2 IS NOT NULL)"
            }
            IntakeView::All => "(?2 IS NOT NULL)",
        };
        sqlx::query_as(&format!(
            "SELECT {ENTRY_COLS} {ENTRY_FROM} WHERE i.project_id = ?1 AND {cond} AND (?3 IS NULL OR w.created_by = ?3)
             ORDER BY i.created_at DESC LIMIT ?4"
        ))
        .bind(project)
        .bind(&now)
        .bind(only_by)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn intake_entry(&self, project: Uuid, item: Uuid) -> Result<Option<IntakeEntry>> {
        sqlx::query_as(&format!("SELECT {ENTRY_COLS}, w.description_html {ENTRY_FROM} WHERE i.project_id = ? AND i.work_item_id = ?"))
            .bind(project)
            .bind(item)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)
    }

    /// Open intake submissions (for the tab's badge).
    pub async fn intake_open_count(&self, project: Uuid, only_by: Option<Uuid>) -> Result<i64> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM intake_items i JOIN work_items w ON w.id = i.work_item_id AND w.deleted_at IS NULL
             WHERE i.project_id = ?1 AND (i.status = 'pending' OR (i.status = 'snoozed' AND i.snoozed_until <= ?2)) AND (?3 IS NULL OR w.created_by = ?3)",
        )
        .bind(project)
        .bind(now_ts())
        .bind(only_by)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)
    }

    /// Triage. `action` is `accept`, `decline`, `duplicate` (with `duplicate_of`), `snooze`
    /// (with `until`) or `reopen`. Returns the submitter to tell and the new status.
    pub async fn triage(
        &self,
        project: Uuid,
        item: Uuid,
        by: Uuid,
        action: &str,
        reason: Option<&str>,
        duplicate_of: Option<Uuid>,
        until: Option<&str>,
    ) -> Result<IntakeEntry> {
        let entry = self
            .intake_entry(project, item)
            .await?
            .ok_or_else(|| Error::not_found("intake submission not found"))?;
        if entry.status == "accepted" {
            return Err(Error::conflict(
                "this submission was already accepted into the project",
            ));
        }
        let now = now_ts();
        let status = match action {
            "accept" => "accepted",
            "decline" => "declined",
            "duplicate" => "duplicate",
            "snooze" => "snoozed",
            "reopen" => "pending",
            _ => return Err(Error::bad_request("unknown intake action")),
        };
        if action == "duplicate" {
            let d = duplicate_of
                .ok_or_else(|| Error::bad_request("say which work item it duplicates"))?;
            let ok: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM work_items WHERE id = ? AND project_id = ? AND deleted_at IS NULL AND is_draft = 0")
                .bind(d)
                .bind(project)
                .fetch_one(self.pool())
                .await
                .map_err(db_err)?;
            if ok == 0 || d == item {
                return Err(Error::bad_request(
                    "the duplicate must be a work item of this project",
                ));
            }
        }
        if action == "snooze" && until.is_none_or(|u| u <= now.as_str()) {
            return Err(Error::bad_request("snooze until a time in the future"));
        }
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        sqlx::query(
            "UPDATE intake_items SET status = ?, reason = ?, duplicate_of = ?, snoozed_until = ?, triaged_by = ?, triaged_at = ? WHERE work_item_id = ?",
        )
        .bind(status)
        .bind(if action == "decline" { reason } else { None })
        .bind(if action == "duplicate" { duplicate_of } else { None })
        .bind(if action == "snooze" { until } else { None })
        .bind(if action == "reopen" { None } else { Some(by) })
        .bind(if action == "reopen" { None } else { Some(now.clone()) })
        .bind(item)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        if action == "accept" {
            sqlx::query(
                "UPDATE work_items SET is_draft = 0, updated_at = ? WHERE id = ? AND is_draft = ?",
            )
            .bind(&now)
            .bind(item)
            .bind(INTAKE)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }
        if action == "duplicate" {
            sqlx::query("INSERT OR IGNORE INTO work_item_relations (id, from_id, to_id, kind, created_by, created_at) VALUES (?, ?, ?, 'duplicate_of', ?, ?)")
                .bind(new_id())
                .bind(item)
                .bind(duplicate_of)
                .bind(by)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }
        sqlx::query(
            "INSERT INTO work_item_activity (id, work_item_id, actor_id, verb, field, old_value, new_value, created_at) VALUES (?, ?, ?, 'updated', 'intake', ?, ?, ?)",
        )
        .bind(new_id())
        .bind(item)
        .bind(by)
        .bind(&entry.status)
        .bind(status)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        if action == "accept" {
            self.index_work_item(item).await?;
        }
        if action != "snooze" {
            self.notify(
                item,
                by,
                &[entry.created_by],
                &Note {
                    kind: "updated",
                    field: Some("intake"),
                    old: Some(&entry.status),
                    new: Some(status),
                    comment: None,
                },
            )
            .await?;
        }
        self.intake_entry(project, item)
            .await?
            .ok_or_else(|| Error::not_found("intake submission not found"))
    }

    /// Turns the public form on (with a new link token) or off (`None`).
    pub async fn set_intake_form(&self, project: Uuid, token: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE projects SET intake_form_token = ? WHERE id = ?")
            .bind(token)
            .bind(project)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn intake_form_token(&self, project: Uuid) -> Result<Option<String>> {
        let t: Option<Option<String>> =
            sqlx::query_scalar("SELECT intake_form_token FROM projects WHERE id = ?")
                .bind(project)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err)?;
        Ok(t.flatten())
    }

    /// (workspace id, workspace name, project id, project name) of an open form.
    pub async fn intake_form(&self, token: &str) -> Result<Option<(Uuid, String, Uuid, String)>> {
        sqlx::query_as(
            "SELECT w.id, w.name, p.id, p.name FROM projects p JOIN workspaces w ON w.id = p.workspace_id
             WHERE p.intake_form_token = ? AND p.feature_intake = 1 AND p.archived_at IS NULL AND p.deleted_at IS NULL AND w.deleted_at IS NULL",
        )
        .bind(token)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    /// The account that form submissions are created by (see [`Store::service_bot`]).
    pub async fn intake_form_user(&self, ws: Uuid) -> Result<Uuid> {
        self.service_bot(ws, "Intake form").await
    }

    /// A named bot of the workspace that is not a member of it (it cannot sign in or see
    /// anything); integrations act as it so their changes show under its name.
    pub async fn service_bot(&self, ws: Uuid, name: &str) -> Result<Uuid> {
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM users WHERE bot_workspace_id = ? AND is_bot = 1 AND name = ? AND deleted_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM workspace_members m WHERE m.user_id = users.id)",
        )
        .bind(ws)
        .bind(name)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        if let Some(id) = existing {
            return Ok(id);
        }
        let id = new_id();
        let now = now_ts();
        sqlx::query("INSERT INTO users (id, email, name, is_bot, bot_workspace_id, created_at, updated_at) VALUES (?, ?, ?, 1, ?, ?, ?)")
            .bind(id)
            .bind(format!("service-{}@bots.nextplane.invalid", id.simple()))
            .bind(name)
            .bind(ws)
            .bind(&now)
            .bind(&now)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(id)
    }
}
