//! The permission rules of docs/PERMISSIONS.md. Pure functions: callers load the roles and
//! facts (who owns what, how many owners are left) and ask here. Every route checks through
//! this module; nothing else decides access.

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

// ---------------------------------------------------------------------------------------------
// Workspace
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WsRole {
    Guest,
    Member,
    Admin,
    Owner,
}

impl WsRole {
    pub fn as_str(self) -> &'static str {
        match self {
            WsRole::Guest => "guest",
            WsRole::Member => "member",
            WsRole::Admin => "admin",
            WsRole::Owner => "owner",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "guest" => Ok(WsRole::Guest),
            "member" => Ok(WsRole::Member),
            "admin" => Ok(WsRole::Admin),
            "owner" => Ok(WsRole::Owner),
            _ => Err(Error::bad_request(format!("unknown workspace role {s:?}"))),
        }
    }

    /// Owner or admin: manages the workspace and is an implicit admin of every project.
    pub fn is_admin(self) -> bool {
        self >= WsRole::Admin
    }
}

/// Facts about the workspace a decision may depend on.
#[derive(Debug, Clone, Copy, Default)]
pub struct WsFacts {
    pub members_can_create_projects: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsAction {
    View,
    ViewMembersPage,
    EditSettings,
    Delete,
    Invite,
    CreateProject,
    BrowsePublicProjects,
    SeeAllPrivateProjects,
}

pub fn ws_can(role: WsRole, action: WsAction, facts: WsFacts) -> bool {
    use WsAction::*;
    match action {
        View => true,
        ViewMembersPage | BrowsePublicProjects => role >= WsRole::Member,
        EditSettings | Invite | SeeAllPrivateProjects => role.is_admin(),
        Delete => role == WsRole::Owner,
        CreateProject => {
            role.is_admin() || (role == WsRole::Member && facts.members_can_create_projects)
        }
    }
}

pub fn ws_require(role: WsRole, action: WsAction, facts: WsFacts) -> Result<()> {
    if ws_can(role, action, facts) {
        Ok(())
    } else {
        Err(Error::forbidden("your workspace role does not allow this"))
    }
}

/// Which roles `actor` may hand out in an invite.
pub fn can_invite_as(actor: WsRole, role: WsRole) -> bool {
    match actor {
        WsRole::Owner => role != WsRole::Owner,
        WsRole::Admin => role <= WsRole::Member,
        _ => false,
    }
}

/// May `actor` change someone's role from `from` to `to`? `is_self` is true when the actor
/// changes their own role. `owners_left` counts owners including the target.
pub fn can_change_role(
    actor: WsRole,
    from: WsRole,
    to: WsRole,
    is_self: bool,
    owners_left: usize,
) -> Result<()> {
    if from == to {
        return Ok(());
    }
    // Permission first, so people without the right learn nothing about owners.
    let allowed = match actor {
        // Owners change anyone's role, their own included; the last-owner rule is above.
        WsRole::Owner => true,
        // Admins move people between member and guest only (PERMISSIONS D1).
        WsRole::Admin => from <= WsRole::Member && to <= WsRole::Member && !is_self,
        _ => false,
    };
    if !allowed {
        return Err(Error::forbidden(
            "your workspace role does not allow this role change",
        ));
    }
    if from == WsRole::Owner && owners_left <= 1 {
        return Err(Error::conflict("a workspace must keep at least one owner"));
    }
    Ok(())
}

/// May `actor` remove a member whose role is `target` (`is_self` = leaving)?
pub fn can_remove_member(
    actor: WsRole,
    target: WsRole,
    is_self: bool,
    owners_left: usize,
) -> Result<()> {
    let allowed = is_self
        || match actor {
            WsRole::Owner => true,
            WsRole::Admin => target <= WsRole::Member,
            _ => false,
        };
    if !allowed {
        return Err(Error::forbidden(
            "your workspace role does not allow removing this member",
        ));
    }
    if target == WsRole::Owner && owners_left <= 1 {
        return Err(Error::conflict("a workspace must keep at least one owner"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Project
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjRole {
    Guest,
    Commenter,
    Contributor,
    Admin,
}

impl ProjRole {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjRole::Guest => "guest",
            ProjRole::Commenter => "commenter",
            ProjRole::Contributor => "contributor",
            ProjRole::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "guest" => Ok(ProjRole::Guest),
            "commenter" => Ok(ProjRole::Commenter),
            "contributor" => Ok(ProjRole::Contributor),
            "admin" => Ok(ProjRole::Admin),
            _ => Err(Error::bad_request(format!("unknown project role {s:?}"))),
        }
    }

    /// The highest project role a workspace role may hold explicitly.
    pub fn ceiling(ws: WsRole) -> ProjRole {
        if ws == WsRole::Guest {
            ProjRole::Commenter
        } else {
            ProjRole::Admin
        }
    }

    /// The role given when someone joins a public project themselves.
    pub fn on_self_join(ws: WsRole) -> Option<ProjRole> {
        match ws {
            WsRole::Owner | WsRole::Admin => Some(ProjRole::Admin),
            WsRole::Member => Some(ProjRole::Contributor),
            WsRole::Guest => None,
        }
    }
}

/// The role that applies in a project: workspace owners/admins are always project admins.
pub fn effective_proj_role(ws: WsRole, explicit: Option<ProjRole>) -> Option<ProjRole> {
    if ws.is_admin() {
        Some(ProjRole::Admin)
    } else {
        explicit
    }
}

/// Whether someone sees a project at all (listed, opened).
pub fn can_see_project(ws: WsRole, explicit: Option<ProjRole>, is_private: bool) -> bool {
    if ws.is_admin() || explicit.is_some() {
        return true;
    }
    !is_private && ws >= WsRole::Member
}

/// Who did a thing, for "own" rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Own,
    Others,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjAction {
    ViewWorkItems,
    /// The project's archives: archived work items, cycles and modules (not guests or
    /// commenters, as in Plane).
    ViewArchives,
    EditSettings,
    ManageMembers,
    ManageStatesLabelsEstimates,
    CreateWorkItem,
    EditWorkItem,
    ArchiveWorkItem,
    DeleteWorkItem(Ownership),
    Comment,
    EditComment(Ownership),
    HideComment,
    React,
    AddAttachment,
    DeleteAttachment(Ownership),
    CreateCycleOrModule,
    EditCycleOrModule,
    DeleteCycleOrModule(Ownership),
    CreateView,
    EditView(Ownership),
    CreatePage,
    EditPage(Ownership),
    SubmitIntake,
    EditIntake(Ownership),
    TriageIntake,
}

pub fn proj_can(role: ProjRole, action: ProjAction) -> bool {
    use Ownership::*;
    use ProjAction::*;
    use ProjRole::*;
    let at_least = |r: ProjRole| role >= r;
    match action {
        ViewWorkItems => at_least(Commenter),
        EditSettings | ManageMembers | ManageStatesLabelsEstimates | HideComment | TriageIntake => {
            role == Admin
        }
        ViewArchives | CreateWorkItem | EditWorkItem | ArchiveWorkItem | AddAttachment
        | CreateCycleOrModule | EditCycleOrModule | CreateView | CreatePage => {
            at_least(Contributor)
        }
        // Project pages are shared documents: every contributor co-edits them (D7). Locking,
        // archiving, deleting and access stay with the owner or an admin.
        EditPage(_) => at_least(Contributor),
        DeleteWorkItem(o) | DeleteAttachment(o) | DeleteCycleOrModule(o) | EditView(o) => {
            role == Admin || (role == Contributor && o == Own)
        }
        Comment => at_least(Commenter),
        // Only the author edits or deletes a comment, admins included (PERMISSIONS §4).
        EditComment(o) => at_least(Commenter) && o == Own,
        React | SubmitIntake => true,
        EditIntake(o) => role == Admin || o == Own,
    }
}

pub fn proj_require(role: Option<ProjRole>, action: ProjAction) -> Result<()> {
    match role {
        Some(r) if proj_can(r, action) => Ok(()),
        Some(_) => Err(Error::forbidden("your project role does not allow this")),
        None => Err(Error::not_found("project not found")),
    }
}

/// May `actor` (effective project role) set someone's explicit role from `from` to `to`?
/// `target_ws` is the target's workspace role, `admins_left` counts explicit project admins
/// including the target.
pub fn can_set_proj_role(
    actor: ProjRole,
    target_ws: WsRole,
    from: Option<ProjRole>,
    to: ProjRole,
    admins_left: usize,
) -> Result<()> {
    if actor != ProjRole::Admin {
        return Err(Error::forbidden(
            "only project admins manage project members",
        ));
    }
    // (permission is checked before the last-admin rule below)
    if to > ProjRole::ceiling(target_ws) {
        return Err(Error::bad_request(
            "a workspace guest can only be a project commenter or guest",
        ));
    }
    if from == Some(ProjRole::Admin) && to != ProjRole::Admin && admins_left <= 1 {
        return Err(Error::conflict("a project must keep at least one admin"));
    }
    Ok(())
}

/// May a project member be removed (or leave, when `is_self`)?
pub fn can_remove_proj_member(
    actor: ProjRole,
    target: ProjRole,
    is_self: bool,
    admins_left: usize,
) -> Result<()> {
    if !is_self && actor != ProjRole::Admin {
        return Err(Error::forbidden(
            "only project admins remove project members",
        ));
    }
    if target == ProjRole::Admin && admins_left <= 1 {
        return Err(Error::conflict("a project must keep at least one admin"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Ownership::*;
    use super::ProjAction::*;
    use super::WsAction::*;
    use super::*;
    use crate::ErrorKind;

    const WS: [WsRole; 4] = [WsRole::Owner, WsRole::Admin, WsRole::Member, WsRole::Guest];
    const PR: [ProjRole; 4] = [
        ProjRole::Admin,
        ProjRole::Contributor,
        ProjRole::Commenter,
        ProjRole::Guest,
    ];

    /// Checks one matrix row: `expected[i]` is the answer for `WS[i]` / `PR[i]`.
    fn ws_row(action: WsAction, facts: WsFacts, expected: [bool; 4]) {
        for (r, e) in WS.iter().zip(expected) {
            assert_eq!(ws_can(*r, action, facts), e, "{action:?} as {r:?}");
        }
    }
    fn pr_row(action: ProjAction, expected: [bool; 4]) {
        for (r, e) in PR.iter().zip(expected) {
            assert_eq!(proj_can(*r, action), e, "{action:?} as {r:?}");
        }
    }
    fn kind<T: std::fmt::Debug>(r: Result<T>) -> Option<ErrorKind> {
        r.err().map(|e| e.kind)
    }

    // §1.1 — order: owner, admin, member, guest
    #[test]
    fn ws_matrix() {
        let off = WsFacts::default();
        let on = WsFacts {
            members_can_create_projects: true,
        };
        ws_row(View, off, [true, true, true, true]);
        ws_row(ViewMembersPage, off, [true, true, true, false]);
        ws_row(WsAction::EditSettings, off, [true, true, false, false]);
        ws_row(Delete, off, [true, false, false, false]);
        ws_row(Invite, off, [true, true, false, false]);
        ws_row(CreateProject, off, [true, true, false, false]);
        ws_row(CreateProject, on, [true, true, true, false]);
        ws_row(BrowsePublicProjects, off, [true, true, true, false]);
        ws_row(SeeAllPrivateProjects, off, [true, true, false, false]);
    }

    #[test]
    fn invite_roles() {
        use WsRole::*;
        assert!(
            can_invite_as(Owner, Admin)
                && can_invite_as(Owner, Member)
                && can_invite_as(Owner, Guest)
        );
        assert!(!can_invite_as(Owner, Owner));
        assert!(can_invite_as(Admin, Member) && can_invite_as(Admin, Guest));
        assert!(!can_invite_as(Admin, Admin), "D1");
        assert!(!can_invite_as(Member, Guest) && !can_invite_as(Guest, Guest));
    }

    #[test]
    fn role_changes() {
        use WsRole::*;
        assert!(can_change_role(Admin, Member, Guest, false, 2).is_ok());
        assert!(can_change_role(Admin, Guest, Member, false, 2).is_ok());
        assert_eq!(
            kind(can_change_role(Admin, Member, Admin, false, 2)),
            Some(ErrorKind::Forbidden),
            "D1"
        );
        assert_eq!(
            kind(can_change_role(Admin, Admin, Member, false, 2)),
            Some(ErrorKind::Forbidden),
            "D1"
        );
        assert_eq!(
            kind(can_change_role(Admin, Owner, Member, false, 2)),
            Some(ErrorKind::Forbidden)
        );
        assert_eq!(
            kind(can_change_role(Member, Guest, Member, false, 2)),
            Some(ErrorKind::Forbidden)
        );
        assert!(can_change_role(Owner, Member, Admin, false, 1).is_ok());
        assert!(can_change_role(Owner, Admin, Owner, false, 1).is_ok());
        assert!(
            can_change_role(Owner, Owner, Admin, false, 2).is_ok(),
            "another owner remains"
        );
        assert!(
            can_change_role(Owner, Owner, Admin, true, 2).is_ok(),
            "step down"
        );
        assert_eq!(
            kind(can_change_role(Admin, Owner, Member, false, 1)),
            Some(ErrorKind::Forbidden),
            "403 before 409"
        );
        assert_eq!(
            kind(can_remove_member(Member, Owner, false, 1)),
            Some(ErrorKind::Forbidden),
            "403 before 409"
        );
        assert_eq!(
            kind(can_change_role(Owner, Owner, Admin, true, 1)),
            Some(ErrorKind::Conflict),
            "last owner"
        );
    }

    #[test]
    fn removals() {
        use WsRole::*;
        assert!(can_remove_member(Owner, Admin, false, 1).is_ok());
        assert!(can_remove_member(Owner, Owner, false, 2).is_ok());
        assert!(can_remove_member(Admin, Member, false, 1).is_ok());
        assert!(can_remove_member(Admin, Guest, false, 1).is_ok());
        assert_eq!(
            kind(can_remove_member(Admin, Admin, false, 1)),
            Some(ErrorKind::Forbidden),
            "D1"
        );
        assert_eq!(
            kind(can_remove_member(Member, Guest, false, 1)),
            Some(ErrorKind::Forbidden)
        );
        assert!(
            can_remove_member(Guest, Guest, true, 1).is_ok(),
            "anyone may leave"
        );
        assert_eq!(
            kind(can_remove_member(Owner, Owner, true, 1)),
            Some(ErrorKind::Conflict),
            "last owner leaves"
        );
    }

    // §2 — project visibility and roles
    #[test]
    fn project_visibility_and_roles() {
        use ProjRole as P;
        use WsRole::*;
        assert_eq!(effective_proj_role(Admin, None), Some(P::Admin));
        assert_eq!(effective_proj_role(Owner, Some(P::Guest)), Some(P::Admin));
        assert_eq!(effective_proj_role(Member, None), None);
        assert_eq!(
            effective_proj_role(Member, Some(P::Commenter)),
            Some(P::Commenter)
        );

        assert!(can_see_project(Admin, None, true));
        assert!(can_see_project(Member, None, false));
        assert!(!can_see_project(Member, None, true), "D4");
        assert!(can_see_project(Member, Some(P::Guest), true));
        assert!(!can_see_project(Guest, None, false));
        assert!(can_see_project(Guest, Some(P::Guest), false));

        assert_eq!(P::on_self_join(Owner), Some(P::Admin));
        assert_eq!(P::on_self_join(Member), Some(P::Contributor));
        assert_eq!(P::on_self_join(Guest), None, "D3");
        assert_eq!(P::ceiling(Guest), P::Commenter);
        assert_eq!(P::ceiling(Member), P::Admin);
    }

    // §2.1 — order: admin, contributor, commenter, guest
    #[test]
    fn project_matrix() {
        pr_row(ViewWorkItems, [true, true, true, false]);
        pr_row(ProjAction::EditSettings, [true, false, false, false]);
        pr_row(ManageMembers, [true, false, false, false]);
        pr_row(ManageStatesLabelsEstimates, [true, false, false, false]);
        pr_row(CreateWorkItem, [true, true, false, false]);
        pr_row(EditWorkItem, [true, true, false, false]);
        pr_row(ArchiveWorkItem, [true, true, false, false]);
        pr_row(DeleteWorkItem(Own), [true, true, false, false]);
        pr_row(DeleteWorkItem(Others), [true, false, false, false]);
        pr_row(Comment, [true, true, true, false]);
        pr_row(EditComment(Own), [true, true, true, false]);
        pr_row(EditComment(Others), [false, false, false, false]);
        pr_row(HideComment, [true, false, false, false]);
        pr_row(React, [true, true, true, true]);
        pr_row(AddAttachment, [true, true, false, false]);
        pr_row(DeleteAttachment(Own), [true, true, false, false]);
        pr_row(DeleteAttachment(Others), [true, false, false, false]);
        pr_row(CreateCycleOrModule, [true, true, false, false]);
        pr_row(EditCycleOrModule, [true, true, false, false]);
        pr_row(DeleteCycleOrModule(Own), [true, true, false, false]);
        pr_row(DeleteCycleOrModule(Others), [true, false, false, false]);
        pr_row(CreateView, [true, true, false, false]);
        pr_row(EditView(Own), [true, true, false, false]);
        pr_row(EditView(Others), [true, false, false, false]);
        pr_row(CreatePage, [true, true, false, false]);
        pr_row(EditPage(Own), [true, true, false, false]);
        pr_row(EditPage(Others), [true, true, false, false]);
        pr_row(SubmitIntake, [true, true, true, true]);
        pr_row(EditIntake(Own), [true, true, true, true]);
        pr_row(EditIntake(Others), [true, false, false, false]);
        pr_row(TriageIntake, [true, false, false, false]);
    }

    #[test]
    fn project_member_management() {
        use ProjRole as P;
        use WsRole::*;
        assert!(can_set_proj_role(P::Admin, Member, None, P::Admin, 1).is_ok());
        assert_eq!(
            kind(can_set_proj_role(P::Contributor, Member, None, P::Guest, 1)),
            Some(ErrorKind::Forbidden)
        );
        assert_eq!(
            kind(can_set_proj_role(P::Admin, Guest, None, P::Contributor, 1)),
            Some(ErrorKind::BadRequest)
        );
        assert!(can_set_proj_role(P::Admin, Guest, None, P::Commenter, 1).is_ok());
        assert_eq!(
            kind(can_set_proj_role(
                P::Admin,
                Member,
                Some(P::Admin),
                P::Contributor,
                1
            )),
            Some(ErrorKind::Conflict),
            "D6"
        );
        assert!(can_set_proj_role(P::Admin, Member, Some(P::Admin), P::Contributor, 2).is_ok());

        assert!(can_remove_proj_member(P::Admin, P::Contributor, false, 1).is_ok());
        assert!(can_remove_proj_member(P::Guest, P::Guest, true, 1).is_ok());
        assert_eq!(
            kind(can_remove_proj_member(P::Contributor, P::Guest, false, 1)),
            Some(ErrorKind::Forbidden)
        );
        assert_eq!(
            kind(can_remove_proj_member(P::Admin, P::Admin, true, 1)),
            Some(ErrorKind::Conflict)
        );
    }
}
