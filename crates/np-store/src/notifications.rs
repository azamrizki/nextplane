//! Notifications (the inbox) and draft work items.
//!
//! Notifications are made from the activity rows a change wrote: subscribers of the work item
//! hear about it, except the person who made the change, and only while they may still read
//! the project. Assignment and mentions go to the person concerned.

use np_core::id::{Uuid, new_id};
use np_core::{Error, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::{Store, db_err, now_ts};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub project_id: Uuid,
    pub work_item_id: Uuid,
    pub actor_id: Uuid,
    pub kind: String,
    pub field: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub comment_id: Option<Uuid>,
    pub created_at: String,
    pub read_at: Option<String>,
    pub archived_at: Option<String>,
    pub snoozed_until: Option<String>,
    pub identifier: String,
    pub sequence: i64,
    pub title: String,
}

/// Who may read work items of a project: active workspace members who are owners/admins or
/// project members above guest.
const CAN_READ: &str = "EXISTS (
    SELECT 1 FROM workspace_members wm JOIN projects p ON p.workspace_id = wm.workspace_id
    WHERE p.id = ?1 AND wm.user_id = u.id AND wm.deactivated_at IS NULL
      AND (wm.role IN ('owner', 'admin') OR EXISTS (
        SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.user_id = u.id
          AND pm.role IN ('admin', 'contributor', 'commenter'))))";

/// Fields whose changes subscribers hear about.
const NOTIFY_FIELDS: &[&str] = &[
    "state",
    "priority",
    "due_date",
    "start_date",
    "title",
    "description",
    "estimate",
    "cycle",
    "module",
    "label",
    "assignee",
    "parent",
];

pub struct Note<'a> {
    pub kind: &'a str,
    pub field: Option<&'a str>,
    pub old: Option<&'a str>,
    pub new: Option<&'a str>,
    pub comment: Option<Uuid>,
}

impl Store {
    /// Sends one notification to each recipient who is not the actor and may read the item.
    pub async fn notify(
        &self,
        item: Uuid,
        actor: Uuid,
        recipients: &[Uuid],
        n: &Note<'_>,
    ) -> Result<u64> {
        let row: Option<(Uuid, Uuid, i64)> = sqlx::query_as(
            "SELECT w.project_id, p.workspace_id, w.is_draft FROM work_items w JOIN projects p ON p.id = w.project_id
             WHERE w.id = ? AND w.deleted_at IS NULL",
        )
        .bind(item)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        let Some((project, ws, hidden)) = row else {
            return Ok(0);
        };
        // Drafts notify nobody; intake submissions only about their triage and discussion.
        let intake_ok = n.field == Some("intake") || matches!(n.kind, "commented" | "mentioned");
        if hidden == crate::work_items::DRAFT || (hidden == crate::work_items::INTAKE && !intake_ok)
        {
            return Ok(0);
        }
        let now = now_ts();
        let mut sent = 0;
        let mut seen = Vec::new();
        for u in recipients {
            if *u == actor || seen.contains(u) {
                continue;
            }
            seen.push(*u);
            let ok: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM users u WHERE u.id = ?2 AND u.is_bot = 0 AND u.deleted_at IS NULL AND {CAN_READ}"))
                .bind(project)
                .bind(u)
                .fetch_one(self.pool())
                .await
                .map_err(db_err)?;
            if ok == 0 {
                continue;
            }
            sqlx::query(
                "INSERT INTO notifications (id, user_id, workspace_id, project_id, work_item_id, actor_id, kind, field, old_value, new_value, comment_id, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(new_id())
            .bind(u)
            .bind(ws)
            .bind(project)
            .bind(item)
            .bind(actor)
            .bind(n.kind)
            .bind(n.field)
            .bind(n.old)
            .bind(n.new)
            .bind(n.comment)
            .bind(&now)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
            sent += 1;
        }
        Ok(sent)
    }

    /// Turns the activity `actor` wrote at `at` on `item` into notifications.
    pub(crate) async fn notify_from_activity(
        &self,
        item: Uuid,
        actor: Uuid,
        at: &str,
    ) -> Result<()> {
        let rows: Vec<ActivityRow> = sqlx::query_as(
            "SELECT verb, field, old_value, new_value FROM work_item_activity WHERE work_item_id = ? AND actor_id = ? AND created_at = ?",
        )
        .bind(item)
        .bind(actor)
        .bind(at)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        if rows.is_empty() {
            return Ok(());
        }
        let subs = self.subscribers(item).await?;
        for (verb, field, old, new) in rows {
            if verb == "archived" {
                self.notify(
                    item,
                    actor,
                    &subs,
                    &Note {
                        kind: "archived",
                        field: None,
                        old: None,
                        new: None,
                        comment: None,
                    },
                )
                .await?;
                continue;
            }
            let Some(f) = field.as_deref() else { continue };
            if !NOTIFY_FIELDS.contains(&f) {
                continue;
            }
            if f == "assignee"
                && verb == "added"
                && let Some(u) = new.as_deref().and_then(|h| Uuid::parse_str(h).ok())
            {
                self.notify(
                    item,
                    actor,
                    &[u],
                    &Note {
                        kind: "assigned",
                        field: None,
                        old: None,
                        new: None,
                        comment: None,
                    },
                )
                .await?;
                let others: Vec<Uuid> = subs.iter().copied().filter(|s| *s != u).collect();
                self.notify(
                    item,
                    actor,
                    &others,
                    &Note {
                        kind: "updated",
                        field: Some(f),
                        old: None,
                        new: new.as_deref(),
                        comment: None,
                    },
                )
                .await?;
                continue;
            }
            self.notify(
                item,
                actor,
                &subs,
                &Note {
                    kind: "updated",
                    field: Some(f),
                    old: old.as_deref(),
                    new: new.as_deref(),
                    comment: None,
                },
            )
            .await?;
        }
        Ok(())
    }

    /// A new comment: mentioned people get "mentioned", other subscribers "commented".
    pub async fn notify_comment(
        &self,
        item: Uuid,
        actor: Uuid,
        comment: Uuid,
        mentioned: &[Uuid],
    ) -> Result<()> {
        self.notify(
            item,
            actor,
            mentioned,
            &Note {
                kind: "mentioned",
                field: Some("comment"),
                old: None,
                new: None,
                comment: Some(comment),
            },
        )
        .await?;
        let others: Vec<Uuid> = self
            .subscribers(item)
            .await?
            .into_iter()
            .filter(|s| !mentioned.contains(s))
            .collect();
        self.notify(
            item,
            actor,
            &others,
            &Note {
                kind: "commented",
                field: None,
                old: None,
                new: None,
                comment: Some(comment),
            },
        )
        .await?;
        Ok(())
    }

    /// Inbox of one user in one workspace. `filter`: unread, all, archived, snoozed.
    pub async fn notifications(
        &self,
        user: Uuid,
        ws: Uuid,
        filter: &str,
        limit: i64,
    ) -> Result<Vec<Notification>> {
        let now = now_ts();
        let cond = match filter {
            "unread" => {
                "n.read_at IS NULL AND n.archived_at IS NULL AND (n.snoozed_until IS NULL OR n.snoozed_until <= ?)"
            }
            "all" => "n.archived_at IS NULL AND (n.snoozed_until IS NULL OR n.snoozed_until <= ?)",
            "archived" => "n.archived_at IS NOT NULL AND ? IS NOT NULL",
            "snoozed" => "n.archived_at IS NULL AND n.snoozed_until > ?",
            _ => {
                return Err(Error::bad_request(
                    "filter is unread, all, archived or snoozed",
                ));
            }
        };
        sqlx::query_as(&format!(
            "SELECT n.id, n.project_id, n.work_item_id, n.actor_id, n.kind, n.field, n.old_value, n.new_value, n.comment_id,
                    n.created_at, n.read_at, n.archived_at, n.snoozed_until, p.identifier, w.sequence, w.title
             FROM notifications n JOIN work_items w ON w.id = n.work_item_id JOIN projects p ON p.id = n.project_id
             WHERE n.user_id = ? AND n.workspace_id = ? AND w.deleted_at IS NULL AND p.deleted_at IS NULL AND {cond}
             ORDER BY n.created_at DESC LIMIT ?"
        ))
        .bind(user)
        .bind(ws)
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn unread_count(&self, user: Uuid, ws: Uuid) -> Result<i64> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications n JOIN work_items w ON w.id = n.work_item_id
             WHERE n.user_id = ? AND n.workspace_id = ? AND n.read_at IS NULL AND n.archived_at IS NULL
               AND (n.snoozed_until IS NULL OR n.snoozed_until <= ?) AND w.deleted_at IS NULL",
        )
        .bind(user)
        .bind(ws)
        .bind(now_ts())
        .fetch_one(self.pool())
        .await
        .map_err(db_err)
    }

    /// `action`: read, unread, archive, unarchive, snooze (with `until`), unsnooze.
    pub async fn mark_notification(
        &self,
        user: Uuid,
        id: Uuid,
        action: &str,
        until: Option<&str>,
    ) -> Result<()> {
        let now = now_ts();
        let set = match action {
            "read" => "read_at = COALESCE(read_at, ?1)",
            "unread" => "read_at = NULL",
            "archive" => "archived_at = ?1, read_at = COALESCE(read_at, ?1)",
            "unarchive" => "archived_at = NULL",
            "snooze" => "snoozed_until = ?2, read_at = NULL",
            "unsnooze" => "snoozed_until = NULL",
            _ => return Err(Error::bad_request("unknown action")),
        };
        // ?3 and ?4 are always used, so every statement takes the same four parameters.
        let sql = format!("UPDATE notifications SET {set} WHERE id = ?3 AND user_id = ?4");
        let r = sqlx::query(&sql)
            .bind(&now)
            .bind(until)
            .bind(id)
            .bind(user)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("notification not found"));
        }
        Ok(())
    }

    pub async fn read_all(&self, user: Uuid, ws: Uuid) -> Result<u64> {
        // Only what the inbox shows: snoozed notifications stay unread until they come back.
        let now = now_ts();
        let r = sqlx::query(
            "UPDATE notifications SET read_at = ?1 WHERE user_id = ?2 AND workspace_id = ?3 AND read_at IS NULL
               AND archived_at IS NULL AND (snoozed_until IS NULL OR snoozed_until <= ?1)",
        )
            .bind(&now)
            .bind(user)
            .bind(ws)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(r.rows_affected())
    }

    // ---- email digest -------------------------------------------------------------------

    /// Unread, unemailed notifications older than `cutoff`, per user who wants email.
    pub async fn digest_batches(
        &self,
        cutoff: &str,
    ) -> Result<Vec<(String, String, Vec<Notification>)>> {
        let users: Vec<(Uuid, String, String)> = sqlx::query_as(
            "SELECT DISTINCT u.id, u.email, u.name FROM notifications n JOIN users u ON u.id = n.user_id
             WHERE n.read_at IS NULL AND n.archived_at IS NULL AND n.emailed_at IS NULL AND n.created_at <= ?
               AND u.email_notifications = 1 AND u.is_bot = 0 AND u.deleted_at IS NULL",
        )
        .bind(cutoff)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        let mut out = Vec::new();
        for (id, email, name) in users {
            let ns: Vec<Notification> = sqlx::query_as(
                "SELECT n.id, n.project_id, n.work_item_id, n.actor_id, n.kind, n.field, n.old_value, n.new_value, n.comment_id,
                        n.created_at, n.read_at, n.archived_at, n.snoozed_until, p.identifier, w.sequence, w.title
                 FROM notifications n JOIN work_items w ON w.id = n.work_item_id JOIN projects p ON p.id = n.project_id
                 WHERE n.user_id = ? AND n.read_at IS NULL AND n.archived_at IS NULL AND n.emailed_at IS NULL AND n.created_at <= ?
                   AND w.deleted_at IS NULL ORDER BY n.created_at LIMIT 100",
            )
            .bind(id)
            .bind(cutoff)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
            if !ns.is_empty() {
                out.push((email, name, ns));
            }
        }
        Ok(out)
    }

    pub async fn mark_emailed(&self, ids: &[Uuid]) -> Result<()> {
        let now = now_ts();
        for id in ids {
            sqlx::query("UPDATE notifications SET emailed_at = ? WHERE id = ?")
                .bind(&now)
                .bind(id)
                .execute(self.pool())
                .await
                .map_err(db_err)?;
        }
        Ok(())
    }

    pub async fn set_email_notifications(&self, user: Uuid, on: bool) -> Result<()> {
        sqlx::query("UPDATE users SET email_notifications = ? WHERE id = ?")
            .bind(on)
            .bind(user)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn email_notifications(&self, user: Uuid) -> Result<bool> {
        sqlx::query_scalar("SELECT email_notifications FROM users WHERE id = ?")
            .bind(user)
            .fetch_one(self.pool())
            .await
            .map_err(db_err)
    }

    // ---- drafts -------------------------------------------------------------------------

    /// Publishes a draft: it joins the project's lists and search, and assignees hear of it.
    pub async fn publish_draft(&self, project: Uuid, id: Uuid, actor: Uuid) -> Result<()> {
        let r = sqlx::query("UPDATE work_items SET is_draft = 0, updated_at = ? WHERE id = ? AND project_id = ? AND is_draft = 1 AND deleted_at IS NULL")
            .bind(now_ts())
            .bind(id)
            .bind(project)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("draft not found"));
        }
        self.index_work_item(id).await?;
        let assignees: Vec<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM work_item_assignees WHERE work_item_id = ?")
                .bind(id)
                .fetch_all(self.pool())
                .await
                .map_err(db_err)?;
        self.notify(
            id,
            actor,
            &assignees,
            &Note {
                kind: "assigned",
                field: None,
                old: None,
                new: None,
                comment: None,
            },
        )
        .await?;
        Ok(())
    }
}

/// (verb, field, old value, new value) of an activity row.
type ActivityRow = (String, Option<String>, Option<String>, Option<String>);
