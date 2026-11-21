//! Projects, project members, states and labels.

use np_core::id::{Uuid, new_id};
use np_core::perm::ProjRole;
use np_core::project::{DEFAULT_STATES, EstimateType, StateGroup};
use np_core::{Error, ErrorKind, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::{Store, db_err, now_ts};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub identifier: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub cover_color: Option<String>,
    pub lead_id: Option<Uuid>,
    pub default_assignee_id: Option<Uuid>,
    pub is_private: bool,
    #[sqlx(try_from = "String")]
    pub estimate_type: EstimateTypeCol,
    #[sqlx(json)]
    pub estimate_values: Vec<String>,
    pub feature_cycles: bool,
    pub feature_modules: bool,
    pub feature_pages: bool,
    pub feature_intake: bool,
    pub created_by: Uuid,
    pub created_at: String,
    pub archived_at: Option<String>,
    /// Automations (months of 30 days, 0 = off): archive closed items, close inactive ones.
    pub archive_in: i64,
    pub close_in: i64,
    /// The cancelled state that auto-close moves items to.
    pub close_state_id: Option<Uuid>,
}

/// `EstimateType` as stored text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EstimateTypeCol(pub EstimateType);

/// Unknown text reads as "none" (sqlx's `try_from` uses the `From` impl).
impl From<String> for EstimateTypeCol {
    fn from(s: String) -> Self {
        EstimateTypeCol(EstimateType::parse(&s))
    }
}

const P_COLS: &str = "p.id, p.workspace_id, p.identifier, p.name, p.description, p.icon, p.cover_color, p.lead_id,
    p.default_assignee_id, p.is_private, p.estimate_type, p.estimate_values, p.feature_cycles, p.feature_modules,
    p.feature_pages, p.feature_intake, p.created_by, p.created_at, p.archived_at, p.archive_in, p.close_in,
    p.close_state_id";

/// A project with the caller's explicit membership role (if any).
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ProjectRow {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub project: Project,
    #[sqlx(try_from = "Option<String>")]
    pub member_role: RoleCol,
    pub member_count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RoleCol(pub Option<ProjRole>);

impl TryFrom<Option<String>> for RoleCol {
    type Error = Error;
    fn try_from(s: Option<String>) -> std::result::Result<Self, Self::Error> {
        Ok(RoleCol(s.map(|s| ProjRole::parse(&s)).transpose()?))
    }
}

#[derive(Debug, Default)]
pub struct NewProject {
    pub identifier: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub cover_color: Option<String>,
    pub lead_id: Option<Uuid>,
    pub is_private: bool,
}

#[derive(Debug, Default)]
pub struct ProjectPatch {
    pub identifier: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<Option<String>>,
    pub cover_color: Option<Option<String>>,
    pub lead_id: Option<Option<Uuid>>,
    pub default_assignee_id: Option<Option<Uuid>>,
    pub is_private: Option<bool>,
    pub estimate: Option<(EstimateType, Vec<String>)>,
    pub feature_cycles: Option<bool>,
    pub feature_modules: Option<bool>,
    pub feature_pages: Option<bool>,
    pub feature_intake: Option<bool>,
    pub archive_in: Option<i64>,
    pub close_in: Option<i64>,
    pub close_state_id: Option<Option<Uuid>>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ProjMember {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub avatar: Option<String>,
    #[sqlx(try_from = "String")]
    pub role: ProjRoleCol,
    #[sqlx(try_from = "String")]
    pub workspace_role: WsRoleCol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ProjRoleCol(pub ProjRole);
impl TryFrom<String> for ProjRoleCol {
    type Error = Error;
    fn try_from(s: String) -> std::result::Result<Self, Self::Error> {
        Ok(ProjRoleCol(ProjRole::parse(&s)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct WsRoleCol(pub np_core::perm::WsRole);
impl TryFrom<String> for WsRoleCol {
    type Error = Error;
    fn try_from(s: String) -> std::result::Result<Self, Self::Error> {
        Ok(WsRoleCol(np_core::perm::WsRole::parse(&s)?))
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct State {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    #[sqlx(try_from = "String", rename = "grp")]
    #[serde(rename = "group")]
    pub group: GroupCol,
    pub sort_order: f64,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct GroupCol(pub StateGroup);
impl TryFrom<String> for GroupCol {
    type Error = Error;
    fn try_from(s: String) -> std::result::Result<Self, Self::Error> {
        Ok(GroupCol(StateGroup::parse(&s)?))
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Label {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub color: String,
    pub sort_order: f64,
}

fn name_taken(what: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |e| match db_err(e) {
        e if e.kind == ErrorKind::Conflict => Error::conflict(format!(
            "a {what} with this name or identifier already exists"
        )),
        e => e,
    }
}

impl Store {
    /// Creates a project with its creator as admin and the default states.
    pub async fn create_project(&self, ws: Uuid, p: &NewProject, creator: Uuid) -> Result<Uuid> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let id = new_id();
        let now = now_ts();
        sqlx::query(
            "INSERT INTO projects (id, workspace_id, identifier, name, description, icon, cover_color, lead_id,
               is_private, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(ws)
        .bind(&p.identifier)
        .bind(&p.name)
        .bind(&p.description)
        .bind(&p.icon)
        .bind(&p.cover_color)
        .bind(p.lead_id)
        .bind(p.is_private)
        .bind(creator)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(name_taken("project"))?;
        sqlx::query("INSERT INTO project_members (project_id, user_id, role, created_at, updated_at) VALUES (?, ?, 'admin', ?, ?)")
            .bind(id)
            .bind(creator)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        for (i, (name, grp, color, is_default)) in DEFAULT_STATES.iter().enumerate() {
            sqlx::query(
                "INSERT INTO states (id, project_id, name, color, grp, sort_order, is_default, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(new_id())
            .bind(id)
            .bind(name)
            .bind(color)
            .bind(grp.as_str())
            .bind((i as f64 + 1.0) * 1000.0)
            .bind(is_default)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(id)
    }

    /// Projects of a workspace with the user's membership, archived ones included or not.
    pub async fn workspace_projects(
        &self,
        ws: Uuid,
        user: Uuid,
        archived: bool,
    ) -> Result<Vec<ProjectRow>> {
        sqlx::query_as(&format!(
            "SELECT {P_COLS}, pm.role AS member_role,
               (SELECT COUNT(*) FROM project_members x WHERE x.project_id = p.id) AS member_count
             FROM projects p LEFT JOIN project_members pm ON pm.project_id = p.id AND pm.user_id = ?
             WHERE p.workspace_id = ? AND p.deleted_at IS NULL AND (p.archived_at IS NOT NULL) = ?
             ORDER BY p.name COLLATE NOCASE"
        ))
        .bind(user)
        .bind(ws)
        .bind(archived)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    /// One live project of a workspace with the user's membership.
    pub async fn project_row(
        &self,
        ws: Uuid,
        project: Uuid,
        user: Uuid,
    ) -> Result<Option<ProjectRow>> {
        sqlx::query_as(&format!(
            "SELECT {P_COLS}, pm.role AS member_role,
               (SELECT COUNT(*) FROM project_members x WHERE x.project_id = p.id) AS member_count
             FROM projects p LEFT JOIN project_members pm ON pm.project_id = p.id AND pm.user_id = ?
             WHERE p.workspace_id = ? AND p.id = ? AND p.deleted_at IS NULL"
        ))
        .bind(user)
        .bind(ws)
        .bind(project)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn update_project(&self, id: Uuid, p: &ProjectPatch) -> Result<()> {
        let (est_set, est_type, est_vals) = match &p.estimate {
            Some((t, v)) => (
                true,
                t.as_str(),
                serde_json::to_string(v).unwrap_or_else(|_| "[]".into()),
            ),
            None => (false, "none", "[]".into()),
        };
        sqlx::query(
            "UPDATE projects SET
               identifier = COALESCE(?, identifier),
               name = COALESCE(?, name),
               description = COALESCE(?, description),
               icon = CASE WHEN ? THEN ? ELSE icon END,
               cover_color = CASE WHEN ? THEN ? ELSE cover_color END,
               lead_id = CASE WHEN ? THEN ? ELSE lead_id END,
               default_assignee_id = CASE WHEN ? THEN ? ELSE default_assignee_id END,
               is_private = COALESCE(?, is_private),
               estimate_type = CASE WHEN ? THEN ? ELSE estimate_type END,
               estimate_values = CASE WHEN ? THEN ? ELSE estimate_values END,
               feature_cycles = COALESCE(?, feature_cycles),
               feature_modules = COALESCE(?, feature_modules),
               feature_pages = COALESCE(?, feature_pages),
               feature_intake = COALESCE(?, feature_intake),
               archive_in = COALESCE(?, archive_in),
               close_in = COALESCE(?, close_in),
               close_state_id = CASE WHEN ? THEN ? ELSE close_state_id END,
               updated_at = ?
             WHERE id = ?",
        )
        .bind(&p.identifier)
        .bind(&p.name)
        .bind(&p.description)
        .bind(p.icon.is_some())
        .bind(p.icon.clone().flatten())
        .bind(p.cover_color.is_some())
        .bind(p.cover_color.clone().flatten())
        .bind(p.lead_id.is_some())
        .bind(p.lead_id.flatten())
        .bind(p.default_assignee_id.is_some())
        .bind(p.default_assignee_id.flatten())
        .bind(p.is_private)
        .bind(est_set)
        .bind(est_type)
        .bind(est_set)
        .bind(est_vals)
        .bind(p.feature_cycles)
        .bind(p.feature_modules)
        .bind(p.feature_pages)
        .bind(p.feature_intake)
        .bind(p.archive_in)
        .bind(p.close_in)
        .bind(p.close_state_id.is_some())
        .bind(p.close_state_id.flatten())
        .bind(now_ts())
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(name_taken("project"))?;
        Ok(())
    }

    pub async fn set_project_archived(&self, id: Uuid, archived: bool) -> Result<()> {
        let now = now_ts();
        sqlx::query("UPDATE projects SET archived_at = CASE WHEN ? THEN ? ELSE NULL END, updated_at = ? WHERE id = ?")
            .bind(archived)
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Deletes a project. The row stays with `deleted_at` (its identifier and name become free);
    /// nothing in the UI or API reads deleted projects.
    pub async fn delete_project(&self, id: Uuid) -> Result<()> {
        let now = now_ts();
        sqlx::query("UPDATE projects SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    // ---- members ------------------------------------------------------------------------

    pub async fn project_members(&self, project: Uuid, ws: Uuid) -> Result<Vec<ProjMember>> {
        sqlx::query_as(
            "SELECT u.id AS user_id, u.email, u.name, u.avatar, pm.role, wm.role AS workspace_role
             FROM project_members pm
             JOIN users u ON u.id = pm.user_id
             JOIN workspace_members wm ON wm.user_id = pm.user_id AND wm.workspace_id = ? AND wm.deactivated_at IS NULL
             WHERE pm.project_id = ? AND u.deleted_at IS NULL
             ORDER BY CASE pm.role WHEN 'admin' THEN 0 WHEN 'contributor' THEN 1 WHEN 'commenter' THEN 2 ELSE 3 END,
                      COALESCE(NULLIF(u.name, ''), u.email) COLLATE NOCASE",
        )
        .bind(ws)
        .bind(project)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn proj_role(&self, project: Uuid, user: Uuid) -> Result<Option<ProjRole>> {
        let r: Option<String> = sqlx::query_scalar(
            "SELECT role FROM project_members WHERE project_id = ? AND user_id = ?",
        )
        .bind(project)
        .bind(user)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        r.map(|r| ProjRole::parse(&r)).transpose()
    }

    /// Explicit admins of a project who are still active in the workspace.
    pub async fn count_project_admins(&self, project: Uuid) -> Result<usize> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM project_members pm JOIN projects p ON p.id = pm.project_id
             JOIN workspace_members wm ON wm.workspace_id = p.workspace_id AND wm.user_id = pm.user_id AND wm.deactivated_at IS NULL
             WHERE pm.project_id = ? AND pm.role = 'admin'",
        )
        .bind(project)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(n as usize)
    }

    pub async fn set_project_member(
        &self,
        project: Uuid,
        user: Uuid,
        role: ProjRole,
    ) -> Result<()> {
        let now = now_ts();
        sqlx::query(
            "INSERT INTO project_members (project_id, user_id, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (project_id, user_id) DO UPDATE SET role = excluded.role, updated_at = excluded.updated_at",
        )
        .bind(project)
        .bind(user)
        .bind(role.as_str())
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn remove_project_member(&self, project: Uuid, user: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM project_members WHERE project_id = ? AND user_id = ?")
            .bind(project)
            .bind(user)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    // ---- states -------------------------------------------------------------------------

    pub async fn states(&self, project: Uuid) -> Result<Vec<State>> {
        sqlx::query_as(
            "SELECT id, name, color, grp, sort_order, is_default FROM states WHERE project_id = ?
             ORDER BY CASE grp WHEN 'backlog' THEN 0 WHEN 'unstarted' THEN 1 WHEN 'started' THEN 2
                      WHEN 'completed' THEN 3 ELSE 4 END, sort_order",
        )
        .bind(project)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)
    }

    pub async fn create_state(
        &self,
        project: Uuid,
        name: &str,
        color: &str,
        group: StateGroup,
    ) -> Result<State> {
        let id = new_id();
        let now = now_ts();
        let max: Option<f64> = sqlx::query_scalar(
            "SELECT MAX(sort_order) FROM states WHERE project_id = ? AND grp = ?",
        )
        .bind(project)
        .bind(group.as_str())
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        let order = max.unwrap_or(0.0) + 1000.0;
        sqlx::query(
            "INSERT INTO states (id, project_id, name, color, grp, sort_order, is_default, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(id)
        .bind(project)
        .bind(name)
        .bind(color)
        .bind(group.as_str())
        .bind(order)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(name_taken("state"))?;
        Ok(State {
            id,
            name: name.into(),
            color: color.into(),
            group: GroupCol(group),
            sort_order: order,
            is_default: false,
        })
    }

    pub async fn update_state(
        &self,
        project: Uuid,
        id: Uuid,
        name: Option<&str>,
        color: Option<&str>,
        group: Option<StateGroup>,
        sort_order: Option<f64>,
        make_default: bool,
    ) -> Result<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        if make_default {
            sqlx::query("UPDATE states SET is_default = 0 WHERE project_id = ?")
                .bind(project)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }
        let r = sqlx::query(
            "UPDATE states SET name = COALESCE(?, name), color = COALESCE(?, color), grp = COALESCE(?, grp),
               sort_order = COALESCE(?, sort_order), is_default = CASE WHEN ? THEN 1 ELSE is_default END, updated_at = ?
             WHERE id = ? AND project_id = ?",
        )
        .bind(name)
        .bind(color)
        .bind(group.map(StateGroup::as_str))
        .bind(sort_order)
        .bind(make_default)
        .bind(now_ts())
        .bind(id)
        .bind(project)
        .execute(&mut *tx)
        .await
        .map_err(name_taken("state"))?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("state not found"));
        }
        tx.commit().await.map_err(db_err)
    }

    /// Deletes a state that is not the default and holds no work items.
    pub async fn delete_state(&self, project: Uuid, id: Uuid) -> Result<()> {
        let st: Option<bool> =
            sqlx::query_scalar("SELECT is_default FROM states WHERE id = ? AND project_id = ?")
                .bind(id)
                .bind(project)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err)?;
        match st {
            None => return Err(Error::not_found("state not found")),
            Some(true) => {
                return Err(Error::conflict(
                    "the default state cannot be deleted; make another state the default first",
                ));
            }
            Some(false) => {}
        }
        if self.state_in_use(id).await? {
            return Err(Error::conflict(
                "move the work items in this state to another state first",
            ));
        }
        sqlx::query("DELETE FROM states WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    // ---- labels -------------------------------------------------------------------------

    pub async fn labels(&self, project: Uuid) -> Result<Vec<Label>> {
        sqlx::query_as("SELECT id, parent_id, name, color, sort_order FROM labels WHERE project_id = ? ORDER BY sort_order")
            .bind(project)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)
    }

    async fn check_label_parent(
        &self,
        project: Uuid,
        parent: Option<Uuid>,
        this: Option<Uuid>,
    ) -> Result<()> {
        let Some(parent) = parent else { return Ok(()) };
        if Some(parent) == this {
            return Err(Error::bad_request("a label cannot be its own parent"));
        }
        // One level of nesting, as in Plane's label groups: the parent must be top level.
        let row: Option<Option<Uuid>> =
            sqlx::query_scalar("SELECT parent_id FROM labels WHERE id = ? AND project_id = ?")
                .bind(parent)
                .bind(project)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err)?;
        match row {
            None => Err(Error::bad_request("parent label not found")),
            Some(Some(_)) => Err(Error::bad_request("labels nest one level deep")),
            Some(None) => Ok(()),
        }
    }

    pub async fn create_label(
        &self,
        project: Uuid,
        name: &str,
        color: &str,
        parent: Option<Uuid>,
    ) -> Result<Label> {
        self.check_label_parent(project, parent, None).await?;
        let id = new_id();
        let now = now_ts();
        let max: Option<f64> =
            sqlx::query_scalar("SELECT MAX(sort_order) FROM labels WHERE project_id = ?")
                .bind(project)
                .fetch_one(self.pool())
                .await
                .map_err(db_err)?;
        let order = max.unwrap_or(0.0) + 1000.0;
        sqlx::query(
            "INSERT INTO labels (id, project_id, parent_id, name, color, sort_order, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(project)
        .bind(parent)
        .bind(name)
        .bind(color)
        .bind(order)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(name_taken("label"))?;
        Ok(Label {
            id,
            parent_id: parent,
            name: name.into(),
            color: color.into(),
            sort_order: order,
        })
    }

    pub async fn update_label(
        &self,
        project: Uuid,
        id: Uuid,
        name: Option<&str>,
        color: Option<&str>,
        parent: Option<Option<Uuid>>,
        sort_order: Option<f64>,
    ) -> Result<()> {
        if let Some(p) = parent {
            self.check_label_parent(project, p, Some(id)).await?;
            if p.is_some() {
                let kids: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM labels WHERE parent_id = ?")
                        .bind(id)
                        .fetch_one(self.pool())
                        .await
                        .map_err(db_err)?;
                if kids > 0 {
                    return Err(Error::bad_request(
                        "a label with sub-labels cannot be nested",
                    ));
                }
            }
        }
        let r = sqlx::query(
            "UPDATE labels SET name = COALESCE(?, name), color = COALESCE(?, color),
               parent_id = CASE WHEN ? THEN ? ELSE parent_id END, sort_order = COALESCE(?, sort_order), updated_at = ?
             WHERE id = ? AND project_id = ?",
        )
        .bind(name)
        .bind(color)
        .bind(parent.is_some())
        .bind(parent.flatten())
        .bind(sort_order)
        .bind(now_ts())
        .bind(id)
        .bind(project)
        .execute(self.pool())
        .await
        .map_err(name_taken("label"))?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("label not found"));
        }
        Ok(())
    }

    /// Deletes a label; its sub-labels move to the top level and work items lose it.
    pub async fn delete_label(&self, project: Uuid, id: Uuid) -> Result<()> {
        let r = sqlx::query("DELETE FROM labels WHERE id = ? AND project_id = ?")
            .bind(id)
            .bind(project)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Err(Error::not_found("label not found"));
        }
        Ok(())
    }
}

impl Store {
    /// Whether any live work item uses this state.
    pub(crate) async fn state_in_use(&self, state: Uuid) -> Result<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM work_items WHERE state_id = ? AND deleted_at IS NULL",
        )
        .bind(state)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(n > 0)
    }
}

impl Store {
    pub async fn project_is_private(&self, id: Uuid) -> Result<Option<bool>> {
        sqlx::query_scalar("SELECT is_private FROM projects WHERE id = ? AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)
    }
}
