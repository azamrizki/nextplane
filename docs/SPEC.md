# NextPlane — Self-Hosted Project Management in One Binary

**Status: draft v0.2 (2026-11-20).** Nothing is built yet. The owner's answers to the open questions are recorded in §4 (decisions 5–10).

NextPlane is a self-hosted project management tool for software teams: work items, cycles, modules, views, pages and analytics. It takes [Plane](https://github.com/makeplane/plane) as its **functional reference** and follows the shape of [NextBoard](../../../nextboard/nextboard): one binary, installed once, run as a service, serving its own web UI and API.

## 1. Goal

1. **One binary, zero external services.** No PostgreSQL, Redis, Celery, MinIO or reverse-proxy container is required. `nextplane` plus a data folder is the whole install.
2. **Plane-class project management.** The core of Plane's feature set (§6), rebuilt in our own code and design.
3. **Fast and light.** Starts in under a second, idles in well under 100 MB of RAM, stays responsive with 100k work items in one workspace.
4. **Real-time by default.** Every change reaches every open client over WebSocket without a refresh; pages are edited collaboratively.
5. **Yours.** Self-hosted, no account with us, no telemetry, data in one folder that is easy to back up.
6. **Agent-ready.** A clean REST API and an MCP server, so AI agents (including NextBoard's) can read, create and update work items (§6.10).

### Non-goals (for now)

- Multi-tenant SaaS, billing, plans or seat limits.
- Android and desktop clients. **Postponed until the web version is stable and final** (owner decision, 2026-11-20). The API is still designed so they can be built later (§13).
- Horizontal scaling across several servers. One process on one machine.
- Copying Plane's code, CSS, icons, illustrations, name, logo or wording (§2). Its look and behaviour are followed on purpose, rebuilt with our own code (owner decision, 2026-11-21).

## 2. Relationship to Plane (license rule)

Plane is **AGPL-3.0**. NextPlane is a clean reimplementation:

- **Allowed:** studying Plane's features, concepts, workflows and public API docs; using the same general concepts (work item, cycle, module, view, page, intake), which are common industry terms.
- **Not allowed:** copying or translating Plane source code, its database schema files, UI components, CSS, icons, illustrations, text or branding.
- **Look and behaviour (owner decision, 2026-11-21):** the web app follows Plane's layout, interaction patterns and visual style (sidebar, headers, list/board anatomy, property chips, colors, sizes, shortcuts) so Plane users feel at home. It is rebuilt from a written description in our own code: values such as colors and sizes may match, but no Plane component, CSS, icon artwork, illustration, text, logo or name is copied.
- Libraries Plane happens to use (React, Tiptap, Yjs, lucide icons…) may be used under their own licenses.
- NextPlane's own license: **MIT** (same as NextBoard).

## 3. Core concepts

| Term | Meaning |
|---|---|
| **Instance** | One running `nextplane` binary with its data folder. Has one instance admin. |
| **Workspace** | Top-level container for a team. Has members with roles. One instance can host several workspaces. |
| **Member** | A user in a workspace with a role: `owner`, `admin`, `member`, `guest`; inside a project: `admin`, `contributor`, `commenter`, `guest` ([PERMISSIONS.md](PERMISSIONS.md)). |
| **Project** | A product or team area inside a workspace, with an identifier (e.g. `WEB`) used in work item keys (`WEB-42`). Has its own members, states, labels and settings. |
| **Work item** | The unit of work (issue/task). Title, rich description, state, priority, assignees, labels, dates, estimate, parent, relations. |
| **State** | A project-defined workflow step, grouped into fixed groups: `backlog`, `unstarted`, `started`, `completed`, `cancelled`. |
| **Cycle** | A time-boxed iteration (sprint) with start and end dates. |
| **Module** | A feature-sized group of work items with a lead, status and target date. |
| **View** | A saved filter + layout + display settings, personal or shared. |
| **Page** | A collaborative rich-text document in a project or workspace. |
| **Intake** | A queue of incoming requests that a triager accepts, declines or marks duplicate. |
| **Activity** | The append-only history of every change to a work item. |

## 4. Decisions already made

| # | Decision | Date |
|---|---|---|
| 1 | Plane is the functional reference; code is written from scratch (§2). | 2026-11-20 |
| 2 | Backend in **Rust**, delivered as **one static binary** that also embeds the web UI. | 2026-11-20 |
| 3 | Frontend in **React** + TypeScript + Vite, like NextBoard. | 2026-11-20 |
| 4 | **Web first.** Android and desktop wait until the web version is stable and final. | 2026-11-20 |
| 5 | Storage in **SQLite** inside the binary, not JSON files (NextBoard's model does not fit relational, multi-user data). **SQLite only for v1**; no Postgres backend. | 2026-11-20 |
| 6 | License **MIT**. | 2026-11-20 |
| 7 | Roles and permissions, guests included, **behave as close as possible to Plane** (Jira where Plane is silent). | 2026-11-20 |
| 8 | Default listen **`127.0.0.1:12313`**, next to NextBoard's `12312` (NextBoard does not use 12313). | 2026-11-20 |
| 9 | **Several workspaces per instance**, like Plane. | 2026-11-20 |
| 10 | The name **NextPlane** is final. It never presents itself as Plane. | 2026-11-20 |
| 11 | SQL uses runtime-checked `sqlx::query` / `query_as`, not the compile-time `query!` macros. Every query is covered by integration tests on a temp database instead; builds need no `DATABASE_URL` or offline query cache. | 2026-11-20 |

## 5. Architecture

```
                 browser (React SPA, embedded in the binary)
                        │  HTTPS (nginx / Caddy / Tailscale)
                        ▼
┌──────────────────────── nextplane (one process) ────────────────────────┐
│  axum HTTP router                                                       │
│   ├─ /api/v1/*      REST (JSON)          ├─ /mcp        MCP server       │
│   ├─ /ws            realtime events      ├─ /collab     page CRDT (yrs)  │
│   └─ /*             static SPA (rust-embed)                              │
│                                                                         │
│  domain services  ── workspaces, projects, work items, cycles, …        │
│  event bus        ── tokio broadcast → WebSocket fan-out, notifications │
│  job runner       ── tokio tasks + `jobs` table (email, webhooks, …)    │
│  search           ── SQLite FTS5                                        │
│  storage          ── SQLite (WAL) + files on disk (uploads)             │
└─────────────────────────────────────────────────────────────────────────┘
                 data folder: ~/.nextplane  (or $NEXTPLANE_HOME)
```

### 5.1 Technology

| Concern | Choice | Notes |
|---|---|---|
| Language | Rust (stable, edition 2024) | |
| Async runtime | `tokio` | |
| HTTP / WebSocket | `axum`, `tower-http` | |
| Database | SQLite via `sqlx` (runtime-checked queries, decision 11), WAL mode | Migrations embedded with `sqlx::migrate!`. SQLite only in v1 (decision 5). |
| Full-text search | SQLite FTS5 | Work items, pages, comments. |
| Collaboration | `yrs` (Yjs port) + y-sync protocol over WebSocket | Client uses `yjs` + Tiptap's collaboration extension. |
| Passwords | `argon2` (argon2id) | |
| Sessions | Opaque random tokens, hashed in DB, `HttpOnly` + `SameSite=Lax` cookie | API tokens for scripts and agents. |
| Embedded assets | `rust-embed` | `web/dist` built before `cargo build`. |
| Config | TOML file + env overrides | |
| Logging | `tracing`, JSON or pretty | |
| Email (optional) | `lettre` over SMTP | Off when no SMTP is configured. |
| Frontend | React 19, TypeScript, Vite, React Router | State: TanStack Query for server data. Editor: Tiptap. Drag-and-drop: `dnd-kit`. |

### 5.2 Repository layout *(proposal)*

```
nextplane/
├── Cargo.toml            # workspace
├── crates/
│   ├── nextplane/        # the binary: CLI, config, server startup
│   ├── np-core/          # domain types, validation, permissions (no I/O)
│   ├── np-store/         # SQLite access, migrations
│   ├── np-api/           # axum routes, auth, WebSocket, MCP
│   ├── np-collab/        # yrs page sync
│   └── np-client/        # typed Rust API client (later reused by Tauri)
├── web/                  # React SPA
├── docs/                 # SPEC.md, API.md, DESIGN.md, ROADMAP.md
├── deploy/               # systemd unit, nginx/Caddy examples
└── Makefile
```

### 5.3 Data folder

```
~/.nextplane/            (mode 0700)
├── config.toml
├── nextplane.db         # + -wal, -shm
├── uploads/<workspace>/<uuid>
├── backups/             # automatic daily snapshots (§10)
└── logs/
```

## 6. Features

Priority: **P1** = MVP (phase 1), **P2** = phase 2, **P3** = phase 3+.

### 6.1 Instance and accounts — P1
- First-run setup link (like NextBoard): create the instance admin and the first workspace.
- Email + password login; invite links; password reset (by email when SMTP is set, otherwise an admin-generated link).
- Instance admin page: users, workspaces, SMTP, signup policy (closed / invite only / open).
- *P2:* OAuth/OIDC login (GitHub, Google, generic OIDC). *S3.7:* sign in with Google, GitHub (optionally only an organization's members), GitLab and Gitea, set up by the instance admin; password sign-in can be turned off (instance admins keep theirs as a way back in).
- *S3.7:* instance admin switches on an AI writing assistant (any OpenAI-compatible Chat Completions API) and Unsplash covers; keys are never sent back to a browser.

### 6.2 Workspaces and members — P1
- Create, rename, delete (soft delete, 30-day trash).
- Roles and their behavior follow Plane as closely as possible (decision 7). The full matrix, checked against Plane's docs on 2026-11-20, is [PERMISSIONS.md](PERMISSIONS.md):
  - Workspace roles `owner`, `admin`, `member`, `guest`; a workspace always keeps one owner.
  - Project roles `admin`, `contributor`, `commenter`, `guest`; workspace owners/admins are implicit project admins; workspace guests can only be project `commenter` or `guest`.
  - Plane's old "guests can view all work items" setting is the `commenter` role.

### 6.3 Projects — P1
- Name, identifier (2–10 uppercase letters, unique per workspace), description, emoji/icon, cover color, lead, default assignee.
- Public / private project.
- Per-project settings: states, labels, estimates (points or t-shirt), enabled features (cycles, modules, pages, intake).
- Archive and restore.
- *S3.7:* cover image (an upload, or an Unsplash photo credited to its photographer); archives of work items, cycles and modules; automations that archive closed work items and close inactive ones after 1–12 months, once a day.

### 6.4 Work items — P1
- Key `PROJ-n` with a per-project sequence that never reuses numbers.
- Fields: title, description (rich text, Tiptap JSON + HTML render), state, priority (`urgent`, `high`, `medium`, `low`, `none`), assignees, labels, start date, due date, estimate, parent, cycle, modules.
- Sub-items (parent/child), relations (`blocks`, `blocked_by`, `relates_to`, `duplicate_of`).
- Comments with mentions and reactions; attachments; links.
- Activity history of every field change.
- Bulk edit from list view.
- Quick-add inline in list and kanban.
- Draft work items *(P2)*; archive and restore.

### 6.5 Layouts and views
- **P1:** List, Kanban (group by state, priority, assignee, label; drag to change), filters, sort, group, display properties.
- **P2:** Spreadsheet, Calendar, Gantt (timeline). Saved views (personal / shared), workspace-wide views ("My work", "Assigned to me", "Created by me").
- *S3.7:* workspace views across projects (All work items, Assigned, Created, Subscribed and custom views) as a spreadsheet; member profiles (summary, work lists, activity) — "Your work" is your own profile; a home page with quick links, recents and stickies.

### 6.6 Cycles — P2
- Upcoming, current, completed. One current cycle per project at a time *(proposal)*.
- Progress by state group, burn-down chart, scope changes.
- On completion, move unfinished items to another cycle.

### 6.7 Modules — P2
- Lead, members, status (`backlog`, `planned`, `in_progress`, `paused`, `completed`, `cancelled`), dates, progress.
- Links and description.

### 6.8 Pages — P3
- Collaborative editing (yrs), presence cursors, version history.
- Nested pages, public/private, lock, archive.
- Turn selected text into a work item; embed work item references.

### 6.9 Notifications and inbox — P2
- In-app inbox for mentions, assignments, state changes on subscribed items.
- Optional email digest; webhooks *(P3)*.

### 6.10 API, MCP and agents — P1 (REST) / P2 (MCP)
- Versioned REST API `/api/v1`, used by the web UI itself. Personal API tokens with scopes.
- MCP server: list/search/create/update work items, comment, move state. Lets NextBoard agents pick up and report on work.
- Agent identity: a token can be marked as a bot user so its activity shows as the agent.

### 6.11 Other — P3
- Intake (triage queue, optional public form).
- Analytics (workspace and project dashboards, custom charts).
- Import (CSV first; Jira, Linear, GitHub Issues later) and export (CSV, JSON). *S3.7:* workspace exports of several projects as CSV, JSON or XLSX, with a history.
- Public boards (read-only project view on a public link). *S3.7:* comments, reactions and votes from signed-in visitors where the project allows them; only comments marked public show.
- GitHub integration (link PRs/commits by `PROJ-n` in messages).
- Command palette (`Ctrl/⌘K`) and keyboard shortcuts *(P1 for the palette)*.

## 7. Data model (outline)

All IDs are UUIDv7 (time-ordered) stored as 16-byte BLOBs *(proposal)*. Timestamps are UTC, RFC 3339 on the wire. Every table has `created_at`, `updated_at`; user-content tables have `created_by`, `deleted_at` (soft delete).

```
users(id, email, name, avatar, password_hash, is_instance_admin, …)
sessions(id, user_id, token_hash, expires_at, user_agent, ip)
api_tokens(id, user_id, name, token_hash, scopes, is_bot, last_used_at)

workspaces(id, slug, name, logo, owner_id)
workspace_members(workspace_id, user_id, role)
invites(id, workspace_id, email, role, token_hash, expires_at)

projects(id, workspace_id, identifier, name, description, icon, lead_id,
         is_private, next_sequence, settings_json, archived_at)
project_members(project_id, user_id, role)
states(id, project_id, name, color, group, sort_order, is_default)
labels(id, project_id, parent_id, name, color, sort_order)

work_items(id, project_id, sequence, title, description_json, description_html,
           state_id, priority, start_date, due_date, estimate, parent_id,
           cycle_id, sort_order, completed_at, archived_at)
work_item_assignees(work_item_id, user_id)
work_item_labels(work_item_id, label_id)
work_item_modules(work_item_id, module_id)
work_item_relations(id, from_id, to_id, kind)
work_item_activity(id, work_item_id, actor_id, field, old_value, new_value, at)
comments(id, work_item_id, author_id, body_json, body_html, edited_at)
reactions(id, target_kind, target_id, user_id, emoji)
attachments(id, workspace_id, target_kind, target_id, file_name, size, mime, path)
subscriptions(work_item_id, user_id)

cycles(id, project_id, name, description, start_date, end_date, owner_id)
modules(id, project_id, name, description, status, lead_id, start_date, target_date)
module_members(module_id, user_id)
views(id, workspace_id, project_id NULL, owner_id, name, is_shared,
      filters_json, display_json, layout)
pages(id, project_id NULL, workspace_id, parent_id, title, ydoc BLOB,
      html_snapshot, is_private, locked, archived_at)
notifications(id, user_id, kind, target_id, actor_id, read_at, snoozed_until)
jobs(id, kind, payload_json, run_at, attempts, last_error, done_at)
```

Ordering within lists/kanban uses fractional `sort_order` (REAL or lexicographic keys) so a drag changes one row.

## 8. API and realtime

- REST under `/api/v1/workspaces/{slug}/projects/{id}/work-items/…`, JSON, cursor pagination (`?cursor=&limit=`), filters as query params. Full contract in `docs/API.md` (to be written).
- Errors: `{ "error": { "code": "not_found", "message": "…" } }` with matching HTTP status.
- Optimistic concurrency: `updated_at` (or `version`) sent back on update; stale writes get `409`.
- `/ws`: one socket per tab, subscribe to workspace/project channels; server pushes `{type, entity, id, data}` events after each committed change. The web client applies them to the TanStack Query cache.
- `/collab/{page_id}`: y-sync protocol, persisted to `pages.ydoc` with debounced writes.

## 9. Security

- Permission checks live in `np-core` and run on **every** endpoint, including WebSocket subscriptions and MCP tools. Tests cover each role × action.
- Default listen `127.0.0.1:12313` (decision 8). HTTPS at nginx/Caddy or a private network, like NextBoard.
- CSRF: `SameSite=Lax` cookies + an `Origin` check on state-changing requests.
- Rate limits on login, signup, password reset and invite endpoints.
- Uploads: size limit (default 25 MB), served with `Content-Disposition` and a strict `Content-Type`; HTML/SVG never served inline.
- Rich text sanitized on the server (`ammonia`) before storing HTML.
- Secrets (SMTP password, OAuth secrets) in `config.toml` with mode 0600, never returned by the API.

## 10. Installation and operations

- Install script like NextBoard's: download release, check SHA-256, place binary in `~/.local/bin`, install a systemd user service (Linux) / LaunchAgent (macOS), print a one-time setup link.
- CLI: `nextplane serve | install | uninstall | status | logs | update | setup-link | backup | restore | admin reset-password`.
- `nextplane update`: download, verify, swap binary (keep `.old`), restart. Migrations run on start; a backup is taken before any migration.
- Backups: daily SQLite online backup into `backups/` (keep 14), plus `nextplane backup <file>` producing one archive with DB + uploads. **Restore never overwrites without a confirmation and a fresh backup of the current state.**
- Targets: Linux and macOS, amd64 and arm64. Docker image as an optional extra.

## 11. Frontend

- SPA served from the binary; routes `/{workspace}/projects/{id}/work-items`, `/…/cycles`, etc.
- Design system of our own (`docs/DESIGN.md`, to be written), light and dark themes, keyboard-first.
- Server data via TanStack Query + WebSocket cache updates; optimistic updates for drags and quick edits.
- Large lists virtualized (`@tanstack/react-virtual`).
- i18n ready from day one (English + Indonesian).
- Responsive down to tablet width; phone is usable but not optimized until the mobile phase.

## 12. Quality and testing

- Rust: unit tests in `np-core`, integration tests against a temp SQLite file, permission matrix tests, `cargo clippy -D warnings`, `cargo fmt`.
- Web: Vitest for logic, Playwright e2e against a throwaway instance with a temp data folder (never the real `~/.nextplane`).
- Performance check: seed 100k work items, list/kanban/filter under 200 ms server time.

## 13. Phases

| Phase | Scope | Done when |
|---|---|---|
| **0 — Skeleton** | Cargo workspace, axum + SQLite + migrations, embedded React app, setup link, login, CI | `make build` gives one binary that serves a logged-in empty page |
| **1 — MVP** | §6.1–6.4, list + kanban (§6.5 P1), comments, activity, realtime, REST API + tokens, command palette, install/update/backup | A team can run a project day to day in NextPlane instead of Plane |
| **2 — Planning** | Cycles, modules, spreadsheet/calendar/Gantt, saved views, notifications/inbox, MCP server, OIDC, uploads polish | Sprint planning and tracking work end to end |
| **3 — Knowledge & insight** | Pages (collab), intake, analytics, import/export, public boards, webhooks, GitHub links | Feature-complete web v1 |
| **4 — Clients** | Desktop (Tauri, reusing `np-client`) and Android (Kotlin + Compose) | Starts only after web v1 is declared stable and final |

## 14. Open questions

None blocking phase 0. Answered on 2026-11-20 (decisions 5–10 in §4): MIT license, SQLite only in v1, Plane-like roles and guest rules, port 12313, several workspaces per instance, the name NextPlane.

The Plane permission matrix was checked and written to [PERMISSIONS.md](PERMISSIONS.md) on 2026-11-20.
