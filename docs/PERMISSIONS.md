# NextPlane permissions

Roles and rules follow Plane's documented behavior as closely as possible (SPEC §4, decision 7), checked against docs.plane.so on 2026-11-20 (pages: *Roles and permissions*, *Member roles*, *Permissions matrix*, *Members and invitations*, *Manage project members*, *Manage projects*). Where Plane's docs contradict each other, the **stricter** reading is used and the choice is marked **(D)** with a note at the end. Plane's plan gating (Free/Pro/Business/Enterprise) does not exist here: every role and setting is available. Custom roles, teamspaces, initiatives and billing roles are out of scope.

The code lives in `crates/np-core/src/perm.rs`; every row below has a test.

## 1. Workspace roles

| Role | Level | Summary |
|---|---|---|
| `owner` | 25 | Full control, including deleting the workspace and ownership transfer. The creator is the first owner. |
| `admin` | 20 | Manages members and settings; implicit project admin everywhere. |
| `member` | 15 | Browses and joins public projects; works inside projects per project role. |
| `guest` | 5 | Sees only projects they were explicitly added to. |

### 1.1 Workspace actions

| Action | owner | admin | member | guest |
|---|---|---|---|---|
| View workspace (name, logo, members list in project pickers) | ✓ | ✓ | ✓ | ✓ |
| Open workspace settings › Members page | ✓ | ✓ | ✓ | — |
| Edit workspace settings (name, logo, slug) | ✓ | ✓ | — | — |
| Delete workspace (to trash) / restore it | ✓ | — | — | — |
| Transfer ownership / assign the `owner` role | ✓ | — | — | — |
| Invite members (any role except owner) | ✓ | ✓ **(D1)** | — | — |
| Change a role: target `member`/`guest` → `member`/`guest` | ✓ | ✓ | — | — |
| Change a role to or from `admin` | ✓ | — **(D1)** | — | — |
| Change an owner's role | ✓ (another owner only) | — | — | — |
| Remove a `member`/`guest` | ✓ | ✓ | — | — |
| Remove an `admin` | ✓ | — **(D1)** | — | — |
| Remove an `owner` | ✓ (another owner only) | — | — | — |
| Create a project | ✓ | ✓ | if *members can create projects* is on (default off) **(D2)** | — |
| Browse all public projects | ✓ | ✓ | ✓ | — |
| See private projects without membership | ✓ | ✓ | — | — |
| Leave the workspace | ✓ unless last owner | ✓ | ✓ | ✓ |

**Last-owner rule.** A workspace always keeps at least one owner: the last owner cannot leave, be removed or be demoted. Leaving or removal deactivates the membership; the person's work stays.

**Removal and project admins.** Removing a workspace member who is the only admin of a project is refused until another project admin exists.

## 2. Project roles

| Role | Summary |
|---|---|
| `admin` | Manages project settings, members, states, labels, estimates; edits and deletes anything in the project. |
| `contributor` | Creates and edits work items, cycles, modules, views, pages (any project page); deletes only their own. |
| `commenter` | Views everything in the project, comments, reacts, submits intake. |
| `guest` | Sees only their own intake submissions; cannot comment. |

**Ceilings.** A workspace `guest` can hold only `commenter` or `guest` in a project. Workspace `owner` and `admin` are implicit project admins on every project, private ones included, even without a membership row.

**Joining.** Public projects can be joined by any workspace member or above; joining maps owner/admin → `admin`, member → `contributor`. Workspace guests cannot self-join **(D3)**. Private projects are joined only by invitation from a project admin. A private project is not listed for workspace members who are not in it **(D4)**.

### 2.1 Project actions

| Action | admin | contributor | commenter | guest |
|---|---|---|---|---|
| See the project and its work items | ✓ | ✓ | ✓ | own intake only **(D5)** |
| Edit project settings, archive/restore, delete | ✓ | — | — | — |
| Change public ↔ private | ✓ | — | — | — |
| Add/remove members, change project roles | ✓ | — | — | — |
| View project member list | ✓ | ✓ | ✓ | ✓ |
| Manage states, labels, estimates | ✓ | — | — | — |
| View states, labels, estimates | ✓ | ✓ | ✓ | ✓ |
| Create work items | ✓ | ✓ | — | — |
| Edit any work item (fields, assignees, relations, bulk edit) | ✓ | ✓ | — | — |
| Archive work items (completed or cancelled ones), see the project's archives | ✓ | ✓ | — | — |
| Delete a work item | any | own | — | — |
| Create comments | ✓ | ✓ | ✓ | — |
| Edit/delete a comment | own only | own only | own only | — |
| Hide/unhide any comment | ✓ | — | — | — |
| Reactions | ✓ | ✓ | ✓ | ✓ |
| Attachments: add | ✓ | ✓ | — | on own intake |
| Attachments: delete | any | own | — | own |
| Cycles, modules: create, edit, archive | ✓ | ✓ | — | — |
| Cycles, modules: delete | any | own | — | — |
| Views: create | ✓ | ✓ | — | — |
| Views: edit/delete | any | own | — | — |
| Pages: read (project pages) | ✓ | ✓ | ✓ | — |
| Pages: create | ✓ | ✓ | — | — |
| Pages: edit content, title, place in the tree (any project page) **(D7)** | ✓ | ✓ | — | — |
| Pages: lock, archive, delete | any | own | — | — |
| Pages: make private / share with project | own | own | — | — |
| Intake: submit, edit/delete own submission | ✓ | ✓ | ✓ | ✓ |
| Intake: accept, decline, mark duplicate | ✓ | — | — | — |
| Leave the project | ✓ unless last admin | ✓ | ✓ | ✓ |

**Pages.** Project pages are shared documents: every contributor and admin co-edits them, live **(D7)**. Locking, archiving, deleting and changing access stay with the page's owner or a project admin. A private page is visible only to its owner (not even admins see it). A locked or archived page is read-only for everyone, its owner included, until it is unlocked or restored.

**Intake.** Guests see only the requests they sent; commenters and up see the whole queue. Submitter emails from the public form are shown only to project admins. Requests change only through intake (submitter while pending or snoozed, admins until accepted), not through the regular work item API.

**Last-admin rule.** A project always keeps at least one explicit admin: the last one cannot leave, be removed or be demoted **(D6)**.

## 3. Instance roles

Separate from workspaces. An **instance admin** (the account created by setup, and anyone they promote) manages users, signup policy, SMTP and creates password-reset links. Being instance admin gives no access to a workspace's content.

## 4. Decisions where Plane's docs disagree

- **D1** — *Members and invitations* says admins "can assign member and guest roles, but not owner or admin roles"; the matrix says admins may assign `admin`. Stricter reading: only owners touch `admin`/`owner` roles.
- **D2** — The matrix gives members no project creation, while a workspace setting can allow it. Kept as a setting, off by default.
- **D3** — One page maps guest → project guest on auto-join; the matrix and *Manage projects* say guests cannot self-join. Guests cannot self-join.
- **D4** — The docs do not say whether a private project's name shows to non-members. It does not.
- **D5** — *Member roles* says guests "cannot see or create work items"; the matrix gives guests their own intake items. Both hold: a guest sees and edits only their own intake submissions.
- **D6** — The docs block the last project admin from leaving or being removed, but say nothing about demotion. Demotion is blocked too.
- **D7** — The matrix limits members to editing their own pages, while Plane's pages are built for live co-editing of public project pages. Owner's decision (2026-11-21): every contributor edits any project page; only the owner or an admin locks, archives, deletes or changes access.
- Comments: the matrix says only the author edits or deletes a comment, "including admins"; admins can hide. Followed as written, over the broader "owners/admins have full access" text.
- Plane's former project setting "guests can view all work items" is replaced by the `commenter` role, as in Plane today.
