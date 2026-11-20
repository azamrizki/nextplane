//! Pages: collaborative documents in a project.

use np_core::id::{Uuid, new_id};
use np_core::{Error, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::{Store, db_err, now_ts};

/// How often a new version is kept while someone edits.
pub const VERSION_EVERY_SECS: i64 = 10 * 60;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Page {
    pub id: Uuid,
    pub project_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub title: String,
    pub access: String,
    pub sort_order: f64,
    pub locked_at: Option<String>,
    pub locked_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
    pub updated_by: Option<Uuid>,
    pub archived_at: Option<String>,
    /// Only on single-page reads.
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// Single-page reads: whether a Yjs document exists yet (if not, the first editor
    /// seeds it from `html`, e.g. for pages created through the API).
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_doc: Option<bool>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PageVersion {
    pub id: Uuid,
    pub title: String,
    pub created_by: Option<Uuid>,
    pub created_at: String,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
}

const COLS: &str = "id, project_id, parent_id, owner_id, title, access, sort_order, locked_at, locked_by, created_at, updated_at, updated_by, archived_at";

impl Store {
    /// Pages of a project the user may see: project pages and their own private ones.
    pub async fn pages(&self, project: Uuid, user: Uuid, archived: bool) -> Result<Vec<Page>> {
        sqlx::query_as(&format!(
            "SELECT {COLS} FROM pages WHERE project_id = ? AND deleted_at IS NULL AND (archived_at IS NOT NULL) = ?
               AND (access = 'project' OR owner_id = ?) ORDER BY sort_order, created_at"
        ))
        .bind(project)
        .bind(archived)
        .bind(user)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn page(&self, project: Uuid, id: Uuid) -> Result<Option<Page>> {
        sqlx::query_as(&format!("SELECT {COLS}, html, ydoc IS NOT NULL AS has_doc FROM pages WHERE id = ? AND project_id = ? AND deleted_at IS NULL"))
            .bind(id)
            .bind(project)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)
    }

    /// (workspace, project) of a live page, by id alone (for the collaboration socket).
    pub async fn page_home(&self, id: Uuid) -> Result<Option<(Uuid, Uuid)>> {
        sqlx::query_as(
            "SELECT workspace_id, project_id FROM pages WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn create_page(
        &self,
        ws: Uuid,
        project: Uuid,
        owner: Uuid,
        title: &str,
        parent: Option<Uuid>,
        access: &str,
    ) -> Result<Page> {
        if let Some(p) = parent {
            self.page(project, p)
                .await?
                .ok_or_else(|| Error::bad_request("parent page not found"))?;
        }
        let id = new_id();
        let now = now_ts();
        let max: Option<f64> = sqlx::query_scalar(
            "SELECT MAX(sort_order) FROM pages WHERE project_id = ? AND parent_id IS ?",
        )
        .bind(project)
        .bind(parent)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        sqlx::query(
            "INSERT INTO pages (id, workspace_id, project_id, parent_id, owner_id, title, access, sort_order, created_at, updated_at, updated_by)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(ws)
        .bind(project)
        .bind(parent)
        .bind(owner)
        .bind(title)
        .bind(access)
        .bind(max.unwrap_or(0.0) + 1000.0)
        .bind(&now)
        .bind(&now)
        .bind(owner)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.page(project, id)
            .await?
            .ok_or_else(|| Error::not_found("page not found"))
    }

    /// A copy of a page (text, live document, access, parent), owned by `by`, placed right
    /// after the original.
    pub async fn duplicate_page(
        &self,
        project: Uuid,
        id: Uuid,
        by: Uuid,
        title: &str,
    ) -> Result<Page> {
        let new = new_id();
        let now = now_ts();
        sqlx::query(
            "INSERT INTO pages (id, workspace_id, project_id, parent_id, owner_id, title, access, ydoc, html, sort_order, created_at, updated_at, updated_by)
             SELECT ?, workspace_id, project_id, parent_id, ?, ?, access, ydoc, html,
                    sort_order + (COALESCE((SELECT MIN(n.sort_order) FROM pages n WHERE n.project_id = o.project_id AND n.parent_id IS o.parent_id
                                              AND n.sort_order > o.sort_order AND n.deleted_at IS NULL), o.sort_order + 2000) - o.sort_order) / 2,
                    ?, ?, ?
             FROM pages o WHERE o.id = ? AND o.project_id = ? AND o.deleted_at IS NULL",
        )
        .bind(new)
        .bind(by)
        .bind(title)
        .bind(&now)
        .bind(&now)
        .bind(by)
        .bind(id)
        .bind(project)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.index_page(new).await?;
        self.page(project, new)
            .await?
            .ok_or_else(|| Error::not_found("page not found"))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_page(
        &self,
        project: Uuid,
        id: Uuid,
        by: Uuid,
        title: Option<&str>,
        html: Option<&str>,
        access: Option<&str>,
        parent: Option<Option<Uuid>>,
        sort_order: Option<f64>,
    ) -> Result<Page> {
        if let Some(Some(p)) = parent {
            self.check_page_parent(project, id, p).await?;
        }
        sqlx::query(
            "UPDATE pages SET title = COALESCE(?, title), html = COALESCE(?, html), access = COALESCE(?, access),
               parent_id = CASE WHEN ? THEN ? ELSE parent_id END, sort_order = COALESCE(?, sort_order),
               updated_at = ?, updated_by = ? WHERE id = ? AND project_id = ?",
        )
        .bind(title)
        .bind(html)
        .bind(access)
        .bind(parent.is_some())
        .bind(parent.flatten())
        .bind(sort_order)
        .bind(now_ts())
        .bind(by)
        .bind(id)
        .bind(project)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        if title.is_some() || html.is_some() {
            self.index_page(id).await?;
        }
        if html.is_some() {
            self.maybe_version(id, by).await?;
        }
        self.page(project, id)
            .await?
            .ok_or_else(|| Error::not_found("page not found"))
    }

    /// A page cannot move under itself or one of its own descendants.
    async fn check_page_parent(&self, project: Uuid, id: Uuid, parent: Uuid) -> Result<()> {
        let mut cur = Some(parent);
        let mut n = 0;
        while let Some(c) = cur {
            if c == id {
                return Err(Error::bad_request("a page cannot be inside itself"));
            }
            let row: Option<Option<Uuid>> = sqlx::query_scalar("SELECT parent_id FROM pages WHERE id = ? AND project_id = ? AND deleted_at IS NULL")
                .bind(c)
                .bind(project)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err)?;
            let Some(next) = row else {
                return Err(Error::bad_request("parent page not found"));
            };
            cur = next;
            n += 1;
            if n > 50 {
                return Err(Error::bad_request("pages nest at most 50 deep"));
            }
        }
        Ok(())
    }

    /// Keeps a version when the last one is older than [`VERSION_EVERY_SECS`].
    async fn maybe_version(&self, id: Uuid, by: Uuid) -> Result<()> {
        let last: Option<String> =
            sqlx::query_scalar("SELECT MAX(created_at) FROM page_versions WHERE page_id = ?")
                .bind(id)
                .fetch_one(self.pool())
                .await
                .map_err(db_err)?;
        let due = match last.as_deref().and_then(crate::parse_ts) {
            None => true,
            Some(t) => (time::OffsetDateTime::now_utc() - t).whole_seconds() >= VERSION_EVERY_SECS,
        };
        if due {
            self.add_page_version(id, Some(by)).await?;
        }
        Ok(())
    }

    pub async fn add_page_version(&self, id: Uuid, by: Option<Uuid>) -> Result<()> {
        sqlx::query(
            "INSERT INTO page_versions (id, page_id, title, html, created_by, created_at) SELECT ?, id, title, html, ?, ? FROM pages WHERE id = ?",
        )
        .bind(new_id())
        .bind(by)
        .bind(now_ts())
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn page_versions(&self, id: Uuid) -> Result<Vec<PageVersion>> {
        sqlx::query_as("SELECT id, title, created_by, created_at FROM page_versions WHERE page_id = ? ORDER BY created_at DESC LIMIT 200")
            .bind(id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }

    pub async fn page_version(&self, id: Uuid, version: Uuid) -> Result<Option<PageVersion>> {
        sqlx::query_as("SELECT id, title, created_by, created_at, html FROM page_versions WHERE id = ? AND page_id = ?")
            .bind(version)
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)
    }

    pub async fn page_ydoc(&self, id: Uuid) -> Result<Option<Vec<u8>>> {
        let v: Option<Option<Vec<u8>>> = sqlx::query_scalar("SELECT ydoc FROM pages WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
        Ok(v.flatten())
    }

    /// Saves the live document with its HTML snapshot, re-indexes it and keeps a version
    /// now and then.
    pub async fn save_page_doc(
        &self,
        id: Uuid,
        ydoc: &[u8],
        html: &str,
        by: Option<Uuid>,
    ) -> Result<()> {
        // Text and search index change together.
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        sqlx::query("UPDATE pages SET ydoc = ?, html = ?, updated_at = ?, updated_by = COALESCE(?, updated_by) WHERE id = ?")
            .bind(ydoc)
            .bind(html)
            .bind(now_ts())
            .bind(by)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        index_page_on(&mut tx, id).await?;
        tx.commit().await.map_err(db_err)?;
        if let Some(by) = by {
            self.maybe_version(id, by).await?;
        }
        Ok(())
    }

    pub async fn set_page_flag(&self, id: Uuid, flag: &str, on: bool, by: Uuid) -> Result<()> {
        let sql = match flag {
            "locked" => {
                "UPDATE pages SET locked_at = CASE WHEN ?1 THEN ?2 ELSE NULL END, locked_by = CASE WHEN ?1 THEN ?3 ELSE NULL END WHERE id = ?4"
            }
            "archived" => {
                "UPDATE pages SET archived_at = CASE WHEN ?1 THEN ?2 ELSE NULL END, updated_by = ?3 WHERE id = ?4"
            }
            "deleted" => {
                "UPDATE pages SET deleted_at = CASE WHEN ?1 THEN ?2 ELSE NULL END, updated_by = ?3 WHERE id = ?4"
            }
            _ => return Err(Error::bad_request("unknown flag")),
        };
        sqlx::query(sql)
            .bind(on)
            .bind(now_ts())
            .bind(by)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if flag != "locked" {
            self.index_page(id).await?;
        }
        Ok(())
    }

    pub(crate) async fn index_page(&self, id: Uuid) -> Result<()> {
        let mut conn = self.pool().acquire().await.map_err(db_err)?;
        index_page_on(&mut conn, id).await
    }

    /// Project pages matching a full-text query in the given projects.
    pub async fn search_pages(
        &self,
        projects: &[Uuid],
        q: &str,
        limit: i64,
    ) -> Result<Vec<(Uuid, Uuid, String, String)>> {
        let Some(m) = crate::search::fts_query(q) else {
            return Ok(vec![]);
        };
        if projects.is_empty() {
            return Ok(vec![]);
        }
        let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT f.ref_id, f.project_id, f.title, snippet(search_fts, 5, '[', ']', '…', 12) FROM search_fts f WHERE f.kind = 'page' AND search_fts MATCH ",
        );
        qb.push_bind(m);
        qb.push(" AND f.project_id IN (");
        let mut sep = qb.separated(", ");
        for p in projects {
            sep.push_bind(*p);
        }
        qb.push(") ORDER BY bm25(search_fts, 0, 0, 0, 0, 5.0, 1.0) LIMIT ")
            .push_bind(limit);
        qb.build_query_as()
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }
}

async fn index_page_on(conn: &mut sqlx::SqliteConnection, id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM search_fts WHERE kind = 'page' AND ref_id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(db_err)?;
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT project_id, title, html FROM pages WHERE id = ? AND deleted_at IS NULL AND archived_at IS NULL AND access = 'project'",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(db_err)?;
    if let Some((pid, title, html)) = row {
        sqlx::query("INSERT INTO search_fts (kind, ref_id, project_id, work_item_id, title, body) VALUES ('page', ?, ?, NULL, ?, ?)")
            .bind(id)
            .bind(pid)
            .bind(title)
            .bind(crate::search::html_text(&html))
            .execute(&mut *conn)
            .await
            .map_err(db_err)?;
    }
    Ok(())
}
