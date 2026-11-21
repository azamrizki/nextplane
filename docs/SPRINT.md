# NextPlane — Phases, Sprints and Task Checklist

The single place to see where NextPlane stands: which phase is active, which sprint runs, and which tasks are done. Scope and decisions live in [SPEC.md](SPEC.md); this file tracks progress only.

**Last updated:** 2026-11-21 · **Active phase:** web v1 review by the owner (see the end of phase 3) · **Active sprint:** — (S3.7 done, not released yet; v0.1.1 released 2026-11-21)

## How to use this file

- `[ ]` open · `[~]` in progress · `[x]` done · `[-]` dropped (say why on the same line).
- A task is **done** only when its code is merged, its tests pass (`make test`), and it works in the built binary — not only in `make dev`.
- Every sprint ends with a short **Review** line: what shipped, what moved to the next sprint, and why.
- Tasks moved to another sprint are marked `[-] moved to S…` here and added there, so nothing disappears.
- Update **Last updated**, **Active phase** and **Active sprint** at the top whenever they change.
- Sprint length: about 1 week of work. Dates are filled in when a sprint starts and ends.
- Android and desktop (phase 4) do not start until the owner declares web v1 stable and final (SPEC §4, decision 4).

## Phase overview

| Phase | Name | Sprints | Status | Progress |
|---|---|---|---|---|
| 0 | Skeleton | S0.1 – S0.2 | ✅ Done | 22 / 22 |
| 1 | MVP | S1.1 – S1.6 | ✅ Done (v0.1.0 released) | 60 / 60 |
| 2 | Planning | S2.1 – S2.5 | ✅ Done | 29 / 29 |
| 3 | Knowledge & insight | S3.1 – S3.5 | ✅ Done | 26 / 26 |
| 3.5 | Familiar UI (Plane look and behaviour) | S3.6 | ✅ Done (v0.1.1 released) | 10 / 10 |
| 3.6 | Plane Community Edition parity | S3.7 | ✅ Done | 10 / 10 |
| 4 | Clients (desktop, Android) | — | ⏸ Waiting for web v1 final | — |

Status icons: ⏳ not started · 🚧 in progress · ✅ done · ⏸ on hold.

---

## Phase 0 — Skeleton

**Goal:** `make build` produces one binary that serves the embedded React app, takes the instance through the setup link, and shows a logged-in empty page.
**Exit criteria:** SPEC §13 phase 0 row; CI green; binary runs from a clean data folder on Linux.

### S0.1 — Repository, server and database

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] Cargo workspace with crates `nextplane`, `np-core`, `np-store`, `np-api` (SPEC §5.2)
- [x] `rust-toolchain.toml`, `rustfmt.toml`, clippy settings (`-D warnings`) — files in place; not run yet, this machine has no rustfmt/clippy (see review)
- [x] Config: `config.toml` + env overrides, data folder `~/.nextplane` / `$NEXTPLANE_HOME` (mode 0700)
- [x] `nextplane serve` with axum on `127.0.0.1:12313`, graceful shutdown
- [x] `tracing` logging (pretty for terminal, JSON option)
- [x] SQLite via `sqlx`, WAL mode, embedded migrations, migration 0001 (`users`, `sessions`, `instance`)
- [x] Automatic DB backup before any migration runs
- [x] `GET /api/v1/health` and `GET /api/v1/version`
- [x] Error type → JSON `{ "error": { "code", "message" } }` (SPEC §8)
- [x] Integration test harness: temp data folder + temp SQLite per test (never `~/.nextplane`)

**Review:** Shipped: Cargo workspace (`nextplane`, `np-core`, `np-store`, `np-api`), `nextplane serve` on `127.0.0.1:12313` with graceful shutdown on SIGINT/SIGTERM, `config.toml` (0600) in a 0700 data folder with env/flag overrides, pretty/JSON logs, SQLite in WAL mode with embedded migration 0001 (`instance`, `users`, `sessions`), `VACUUM INTO` backup before any migration on a non-empty database, `/api/v1/health` + `/api/v1/version`, JSON errors for 404/405, 12 tests on temp data folders. Smoke-tested the debug binary on a scratch data folder.
Found while testing: the backup was skipped when a database had data but no migration record; the rule is now "any table present → back up first".
Carried over: run `cargo fmt --check` and `cargo clippy -D warnings` once rustfmt/clippy are available (moved to S0.2 CI task). Queries use runtime-checked `sqlx::query`; compile-time `query!` macros need an offline query cache (`cargo sqlx prepare`) and are decided in S0.2.


### S0.2 — Web app, auth and build

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] `web/`: React 19 + TypeScript + Vite + React Router + TanStack Query
- [x] Embed `web/dist` with `rust-embed`; SPA fallback route; cache headers for hashed assets
- [x] `Makefile`: `web`, `build`, `test`, `dev` (hub + Vite together, like NextBoard)
- [x] One-time setup link printed on first start (30 min validity), `nextplane setup-link`
- [x] Setup wizard: instance admin account + instance name
- [x] Login / logout with argon2id, session cookie (`HttpOnly`, `SameSite=Lax`), `Origin` check
- [x] Rate limit on login
- [x] Empty logged-in shell page (header, user menu, logout)
- [x] Light and dark theme tokens (start of `docs/DESIGN.md`)
- [x] i18n setup with English + Indonesian
- [x] CI: `cargo fmt --check`, clippy, `cargo test`, `npm test`, build of the binary — `.github/workflows/ci.yml` written; it has not run yet (no remote), and fmt/clippy still cannot run on this machine (carried to S1.1)
- [x] Decide compile-time checked queries vs runtime queries → runtime queries + integration tests (SPEC §4 decision 11)

**Review:** Shipped: React 19 + TS 7 + Vite 8 + React Router 8 + TanStack Query app embedded with `rust-embed` (6.8 MB release binary), SPA fallback with `no-cache` for the shell and one-year immutable caching for hashed assets, `nosniff`/`DENY` headers; `make build|test|lint|dev`; setup link on first start (stderr only, 30 min, single use, replaced by `nextplane setup-link [--listen]`); setup wizard; login/logout with argon2id, 30-day sliding session cookie (`HttpOnly`, `SameSite=Lax`, `Secure` behind HTTPS), timing-safe unknown-email path; `Origin` check on state-changing requests; login/setup rate limits; hourly purge of expired sessions; light/dark/system theme; English + Indonesian with a key-parity test; `docs/DESIGN.md`.
Tests: 28 Rust (store, auth, setup, rate limit, origin, cookies), 9 Vitest (API client, i18n, login/setup routing), 3 Playwright e2e against the release binary on temp data folders. The Playwright harness (`web/e2e`) is the start of S1.6's e2e task.
Found while testing: the setup token was written to the structured log as well; it now goes to stderr only.


---

## Phase 1 — MVP

**Goal:** a team can run a project day to day in NextPlane instead of Plane (SPEC §13).
**Before S1.1:** write `docs/PERMISSIONS.md`, checked against Plane's current docs (SPEC §6.2, §14).

### S1.1 — Workspaces, members and permissions

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [-] First `cargo fmt` + `cargo clippy -D warnings` pass — moved to S1.6: still no rustfmt/clippy on this machine; CI runs them once the repo has a remote
- [x] `docs/PERMISSIONS.md`: workspace and project role matrix, Plane-like (decision 7)
- [x] Permission engine in `np-core`, one test per matrix row
- [x] Workspaces: create, rename, slug, logo, soft delete (30-day trash)
- [x] Workspace members and roles (`admin`, `member`, `guest`), at least one admin kept
- [x] Invites by link (and by email when SMTP is set); accept flow for new and existing users
- [x] Workspace switcher in the web app
- [x] Instance admin page: users, workspaces, signup policy
- [x] Password reset (admin-generated link; email when SMTP is set)
- [x] Profile settings: name, password, theme, language; avatar URL via the API. Avatar *upload* moved to S1.4 (needs attachments)

**Review:** Shipped: `docs/PERMISSIONS.md` from Plane's current docs (workspace owner/admin/member/guest, project admin/contributor/commenter/guest, six documented decisions where Plane's docs disagree) and `np-core::perm` with a test for every matrix row; migration 0003; workspaces (create with slug check, rename, slug change, logo URL, "members can create projects", owner-only delete to a 30-day trash with restore and hourly purge); members with role changes and removal under the matrix, last-owner rule, leave; invites in batches with copyable single-use 7-day links, email when `[smtp]` is configured, revoke, newer-replaces-older, accept for existing accounts and signup through the invite; signup policy closed/invite/open; open signup; forgot (always 204) and reset links (email 1 h, admin-made 24 h) that end every session; profile and password change (other sessions end); instance admin page (name, policy, email status, users with admin toggle and reset links, workspaces); `public_url` setting; app frame with sidebar, workspace switcher and user menu; English + Indonesian for all of it.
Tests: 48 Rust (12 matrix tests), 11 Vitest, 8 Playwright e2e on the release binary.
Found while testing: permission errors came after the last-owner check, so an admin could learn that someone was the last owner (409 instead of 403) — permission is now checked first, in both workspace and project rules.


### S1.2 — Projects, states and labels

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] Projects: name, identifier (unique per workspace), description, icon, lead, default assignee
- [x] Public / private projects; join public project
- [x] Project members and roles `admin`/`contributor`/`commenter`/`guest`, guest ceiling, last-admin rule
- [x] States with the five fixed groups, default set on project creation, reorder, default state
- [x] Labels (with parent label), colors, reorder
- [x] Estimates setting (points / t-shirt)
- [x] Feature toggles per project (cycles, modules, pages, intake)
- [x] Archive and restore project
- [x] Project list and project settings pages

**Review:** Shipped: migration 0004; projects (identifier 1–12 letters/digits with a suggestion from the name, unique name and identifier among live projects, emoji icon, color, lead, default assignee, public/private), creator becomes project admin, Plane's five default states (Todo is default); project list with Active/Archived and Join, private projects hidden from non-members (D4), workspace owners/admins as implicit project admins; project members with the guest ceiling (commenter max), last-admin rules for leave/demote/remove (D6), and the workspace refusing to remove a project's only admin; states per group with rename, color, default, reorder, delete (not the default); labels with one level of nesting, delete moves children up; estimates (points, t-shirt, custom values, validated); feature toggles; archive/unarchive; delete (identifier and name become free); project section in the sidebar; settings in six tabs.
Tests: 55 Rust (5 new project API tests covering the matrix, joining, ceilings and last-admin rules), 11 Vitest, 10 Playwright.
Found while testing: feature switches lagged a round-trip behind the click (now optimistic with rollback); e2e spec files sharing a worker shared one server (now one server per file).


### S1.3 — Work items core

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] Work item table + per-project sequence (`PROJ-n`, numbers never reused)
- [x] Create / edit / archive / delete (soft) work items
- [x] Fields: state, priority, assignees, labels, start date, due date, estimate
- [x] Rich text description (Tiptap), server-side sanitizing with `ammonia`
- [x] Parent / sub-items
- [x] Relations: blocks, blocked by, relates to, duplicate of
- [x] Activity history for every field change
- [x] Project role rules on work items: contributor edits any / deletes own, commenter read-only, guest sees own intake only (PERMISSIONS.md §2)
- [x] Work item detail page and peek panel
- [x] Quick-add work item

**Review:** Shipped: migration 0005; work items with a per-project sequence (`WEB-12`, numbers never reused), default state and default assignee, title/description/state/priority/assignees/labels/start/due/estimate/parent, `completed_at` from the state group; Tiptap editor (bold, italic, strike, code, heading, lists, task lists, quote, code block, links) with HTML sanitized server-side by `ammonia` and the JSON kept for editing; parents with loop checks, sub-items, relations (blocks / blocked by / relates to / duplicate of, seen from both sides); activity per changed field; archive/restore; delete (own for contributors, any for admins); filters (state, group, priority, assignee, unassigned, label, creator, parent, due range, search by title or key) and bulk edit on the server for S1.4; key lookup `WEB-12`; archived projects read-only; list grouped by state with quick add, peek panel, full page, create dialog (`C`), property pickers with search and keyboard.
Tests: 65 Rust (6 work item API tests, sanitizer tests), 11 Vitest, 13 Playwright (3 new for work items).
Found while testing: a body that does not parse got axum's plain-text error instead of our JSON format — all routes now use JSON-error `Json`/`Query`/`Path` extractors; the list query failed (500) because the row type required description columns it did not select; empty label chips showed as blank boxes in rows.


### S1.4 — Layouts, filters and comments

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] List layout with grouping (state, priority, assignee, label) and virtualized rows
- [x] Kanban layout, drag to change group, fractional `sort_order`
- [x] Filters (state, priority, assignee, label, dates, created by) and sort
- [x] Display properties toggle
- [x] Bulk edit in list layout
- [x] Comments with mentions, edit, delete
- [x] Reactions on comments and work items
- [x] Attachments (local disk, size limit, safe `Content-Type` / `Content-Disposition`), and avatar/logo upload (from S1.1)
- [x] Links on work items
- [x] Subscriptions (auto-subscribe creator, assignees, commenters)

**Review:** Shipped: migration 0006; list layout grouped by state/priority/assignee/label/none, collapsible groups, virtualized rows (`@tanstack/react-virtual`); kanban with drag between columns and manual reordering (`dnd-kit`, mouse and keyboard), drop patches per grouping; ordering (manual, priority, due, newest, last updated); server-side filters with a filter bar (state, priority, assignee incl. unassigned, label, due overdue/this week, creator, search); display settings (properties, sub-items, empty groups) remembered per project; multi-select with a bulk bar (state, priority, assignees, labels, due, archive); comments with @mentions, edit/delete by the author only, hide by admins, reactions on items and comments; attachments with drag-and-drop upload, served only to people who can see the project, inline only for safe types and with `nosniff` + a sandbox CSP; images pasted or dropped into descriptions and comments; links; subscribe/unsubscribe with automatic subscription for creator, assignees, commenters and mentioned members; avatar and workspace logo upload.
Tests: 71 Rust (5 collaboration API tests), 16 Vitest (5 view-logic tests), 15 Playwright.
Found while testing: quick add cleared the field only after the server answered, so a title typed right after Enter was lost — the field now clears at once and restores on error; filter buttons and row chips shared the name "Priority" and row checkboxes were labelled "Select all" — now "Filter: Priority" and "Select WEB-1".


### S1.5 — Realtime, search and API

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] `/ws`: subscribe to workspace/project channels, permission-checked
- [x] Event bus (tokio broadcast) → WebSocket fan-out after each commit
- [x] Web client applies events to the TanStack Query cache; optimistic updates
- [x] Optimistic concurrency (`409` on stale write) and conflict message in the UI
- [x] Full-text search with FTS5 (work items, comments)
- [x] Command palette (`Ctrl/⌘K`) and core keyboard shortcuts
- [x] Personal API tokens with scopes; bot flag
- [x] `docs/API.md` for every `/api/v1` endpoint used so far
- [x] Cursor pagination on work item lists (default 1000, `X-Next-Cursor`); the other lists are small and bounded (members, states, labels, comments of one item) and return everything — documented in API.md
- [x] Rate limits on signup, invite and reset endpoints (built in S1.1; also on opening invite links)

**Review:** Shipped: migration 0007; event bus (tokio broadcast) fed by a middleware that publishes every successful change under a workspace, `/api/v1/ws` with per-workspace subscription and per-event visibility checks (private projects stay private), `resync` when a client falls behind; the web app keeps one socket per tab with reconnect/backoff and refetches the area that changed, skipping its own echo (`X-Client-Id`); full-text search with FTS5 over titles, descriptions and comments (prefix match, injection-safe query building, index kept current on create/edit/delete/hide); `/search` across readable projects plus project names and `WEB-12` keys; command palette (`Ctrl/⌘K`) and a shortcut list (`?`); personal API tokens (`np_…`, read/write, optional expiry, last-used), bots as workspace members for agents (create, rotate, remove); keyset pagination with the web app fetching all pages; text-edit conflicts (`409`) only when someone else changed the title or description, serialized saves on the client; `docs/API.md`.
Tests: 78 Rust (5 new: tokens and scopes, bots, search, pagination + conflicts, events), 18 Vitest, 19 Playwright (two browsers seeing each other's changes live, palette, tokens and bots).
Found while testing: the first conflict design compared the whole item's `updated_at`, so someone changing only the priority would have rejected your description — conflicts now count only other people's title/description changes; the change-publishing middleware looked up the user on every request, reads included — now only on changes.


### S1.6 — Operations and MVP release

**Dates:** 2026-11-20 → 2026-11-21 · **Status:** ✅

- [x] First `cargo fmt` + `cargo clippy -D warnings` pass (from S1.1) — done 2026-11-21 after the owner had rustfmt/clippy installed: 1,227 formatting fixes, 43 clippy findings fixed (collapsible ifs, complex types, an infallible `TryFrom`, MSRV raised to 1.89 for `File::try_lock`)
- [x] `nextplane install` / `uninstall` (systemd user service, LaunchAgent)
- [x] `status`, `logs`, `admin reset-password` commands
- [x] Daily online backup (keep 14) and `nextplane backup <file>`
- [x] `nextplane restore` with confirmation and a fresh backup of the current state first
- [x] `nextplane update` (download, SHA-256 check, keep `.old`, restart)
- [x] `install.sh` and release workflow (Linux/macOS, amd64/arm64) — written and syntax-checked; not run yet (no GitHub repository)
- [x] `deploy/` examples: systemd unit, nginx and Caddy with HTTPS
- [x] Performance check: 100k seeded work items, list/kanban/filter < 200 ms server time (worst 41 ms; `cargo test -p np-store --release --test perf -- --ignored`)
- [x] Playwright e2e for the main flows against a throwaway instance
- [x] README with quick start
- [x] Tag `v0.1.0` — tagged and released on 2026-11-21 at the owner's go-ahead: https://github.com/azamrizki/nextplane/releases/tag/v0.1.0 (four archives and `SHA256SUMS`; the Linux amd64 download was checked against the sums and runs)

**Review:** Shipped: `install`/`uninstall` (systemd user unit, LaunchAgent, `--dry-run`), `status`, `logs`, `admin reset-password`; a run lock so a second server or a restore cannot touch a data folder in use; daily database copy (newest 14 `daily-*.db` kept, nothing else touched) plus the backup before every migration; `backup` (database, uploads, config in one `.tar.gz`, safe while running); `restore` (refused while running, verifies the archive and its schema, asks for confirmation, writes a fresh backup first and moves replaced files to `backups/replaced-*` instead of deleting); `update` with SHA-256 check and `nextplane.old`; `install.sh`, release workflow, `deploy/` (systemd, nginx, Caddy), README, LICENSE.
Tests: 83 Rust + 1 performance test, 18 Vitest, 19 Playwright.
Found while testing: listing work items took ~9.5 s per page at 100k items — a sub-item count subquery made SQLite scan the table once per item; counts are now computed for the returned page only and a list-order index was added (migration 0008): worst case 41 ms. `status` ignored `NEXTPLANE_LISTEN`.
Open: none. The fmt/clippy pass and the `v0.1.0` release were done on 2026-11-21.

---

## Phase 2 — Planning

**Goal:** sprint planning and tracking end to end (SPEC §13): cycles, modules, more layouts, saved views, an inbox, and agents through MCP.

### S2.1 — Cycles

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] Cycles table, `cycle_id` on work items (one cycle per item), feature toggle respected
- [x] Create/edit/delete (own for contributors), archive; dates may not overlap other cycles in the project
- [x] Status from dates: draft, upcoming, current, completed; completed cycles take no new items
- [x] Progress: items and estimate points per state group
- [x] Burn-down data (ideal vs. remaining per day)
- [x] Transfer unfinished items to another cycle when a cycle ends
- [x] Work item property and filter `cycle` (incl. "no cycle"); activity for cycle changes
- [x] Web: cycles list (current / upcoming / completed / draft), cycle page with its items, progress and burn-down chart
- [x] Tests (API matrix, overlap, transfer, burn-down) and e2e

**Review:** Shipped: migration 0009; cycles with dates that may not overlap, status from dates (draft/upcoming/current/completed, UTC days), completed and archived cycles refuse new items, delete keeps the items (contributors delete their own); one cycle per work item as a property, filter (`cycle=…`, `none`) and activity; progress per state group with estimate points; burn-down (ideal line vs. items still open at the end of each day); transfer of unfinished items; project tabs (Work items / Cycles / Modules); cycles list by status with completion meters; cycle page reusing the list/kanban view scoped to the cycle, with a progress panel and a burn-down chart built to the dataviz rules (colors validated in light and dark, 2px lines, hairline grid, legend + end label, crosshair tooltip by pointer and keyboard, data table).
Tests: 86 Rust (2 cycle API tests, 1 rule test), 18 Vitest, 21 Playwright.
Found while testing: creating or editing your own items did not refresh cycle progress (realtime skips your own echo) — mutations now refresh derived data; menus opened from list rows were covered by the next virtualized row and could not be clicked — the row with an open menu is lifted; a tick showed "3" for 2.5 and the ideal line had rounding kinks.


### S2.2 — Modules

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] Modules with lead, members, status, start/target dates, description, links
- [x] Work items in many modules; property and filter `module`
- [x] Progress per state group; delete own for contributors
- [x] Web: modules list and module page
- [x] Tests and e2e

**Review:** Shipped: migration 0010; modules with status (backlog, planned, in progress, paused, completed, cancelled), lead (the creator by default), members, start/target dates, description and links, unique names per project; work items in any number of modules as a property, filter (`module=…`, `none`) and activity; archived modules take no new items; delete keeps the items; progress and burn-down shared with cycles (`progress_of`); module cards with meters; module page reusing the scoped list/kanban view with progress, burn-down and links.
Tests: 87 Rust (module API test), 18 Vitest, 22 Playwright.


### S2.3 — Layouts and views

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] Spreadsheet layout (inline-editable columns)
- [x] Calendar layout (by due date, drag to reschedule)
- [x] Gantt / timeline layout (start–due bars, drag to move or resize)
- [x] Saved views: personal or shared, filters + layout + display; edit own, admins any
- [x] Workspace views: "My work" (assigned, created, subscribed) across projects
- [x] Tests and e2e

**Review:** Shipped: migration 0011; spreadsheet layout (virtualized, sticky header, every property editable in its cell, horizontal scroll); calendar layout (month grid Mon–Sun, drag to another day moves the due date and keeps the start the same distance before it, items without a due date counted); timeline layout (bars from start to due, drag to move, drag either end to resize, today line, weekends, undated items listed); layout switch as five icon tabs with names for tooltips and screen readers; saved views (filters, layout, grouping, ordering, display) that are personal or shared, apply / update / save as new / delete from a "Views" menu, contributors edit their own and admins any, only the owner (un)shares; "My work" across the projects you have joined (assigned, created, subscribed; open or all).
Tests: 89 Rust (2 view tests), 19 Vitest (date helpers), 27 Playwright (spreadsheet edit, calendar drag, timeline resize, saved views, my work).
Found while testing: saved-view rows nested a delete button inside a menu item (invalid for assistive tech) — split into two buttons; the view toolbar wrapped to two lines at 1280 px; one-day timeline bars showed a clipped single letter. Release builds chained with all tests in one shell command were killed (exit 137); builds now run on their own with `-j 4`.


### S2.4 — Inbox and notifications

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] Notifications for subscribers: assigned, mentioned, state/priority/due changes, comments
- [x] Inbox page: unread/read, archive, snooze, mark all read; live badge
- [x] Optional email digest (SMTP)
- [x] Draft work items (saved, not yet in the project list)
- [x] Tests and e2e

**Review:** Shipped: migration 0012; notifications made from the activity a change writes, sent to the item's subscribers except the person who acted and only while they can still read the project: assignment ("assigned you", also on create and on publishing a draft), property changes (state, priority, dates, title, description, estimate, cycle, modules, labels, parent), archive, new comments ("commented"), and @mentions in comments and descriptions ("mentioned you", only for newly added mentions, replacing the plain comment notice); inbox with unread / all / snoozed / archived, mark read/unread, archive, snooze until tomorrow or next week, mark all read (snoozed ones stay unread); unread badge in the sidebar updated live; email digest every ten minutes when SMTP is set (one email per person, notifications older than ten minutes, never twice, can be turned off in the profile); draft work items (private to the author, not in lists, search, key lookup or cycle/module progress, no notifications) with "Save as draft", a Drafts page and Publish.
Tests: 94 Rust (5 inbox/draft tests incl. digest selection), 19 Vitest, 30 Playwright (live inbox between two browsers, draft → publish).
Found while testing: "mark all read" also marked snoozed notifications, which would come back already read; the "Save as draft" button set React state that the form submit could not see yet — the submit now reads which button was pressed; the mark endpoints rejected a JSON `null` body.


### S2.5 — Agents and sign-in

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅

- [x] MCP server (`/mcp`, streamable HTTP, bearer token): list/search/get/create/update work items, comment, list states/members/cycles
- [x] OIDC sign-in (generic provider in config.toml), link to existing accounts by verified email
- [x] Upload polish: image paste in comments checked end to end, attachment previews
- [x] Tests and e2e

**Review:** Shipped: MCP server at `/mcp` (JSON-RPC over HTTP, protocol 2025-06-18 with older versions accepted, batches, notifications answered 202) with twelve tools — every tool calls the REST API internally with the caller's credentials, so permissions, read-only tokens, activity and notifications behave exactly as in the web app and agents act under their own name; names instead of ids for states, labels and people (`me`), plain-text descriptions and comments escaped to HTML; OIDC single sign-on (discovery, authorization code with PKCE and nonce, single-use ten-minute state, ID token issuer/audience/expiry/nonce checks, userinfo fallback, verified email only, new accounts only with open sign-up or a pending invite, safe `next` redirects) with buttons on the login and invite pages; image paste into comments covered end to end; image attachments open in a preview with a download link.
Tests: 101 Rust (2 MCP, 2 SSO against a local fake provider, OIDC unit tests), 19 Vitest, 31 Playwright.
Found while testing: the setup link was printed before the server listened, so opening it at once failed (now printed after the listener is bound); provider transport errors, with internal URLs, reached the sign-in error page (now logged only).


## Phase 3 — Knowledge & insight

**Goal:** feature-complete web v1 (SPEC §13): collaborative pages, intake, analytics, import/export, public boards, webhooks and GitHub links.

### S3.1 — Pages: collaborative editing

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅ Done

- [x] Pages table (project pages, owner, private/project access, parent, order, lock, archive, delete) and permissions (contributors create and edit their own, admins any; commenters read; private pages only for their owner — widened on 2026-11-21: contributors edit any project page, D7)
- [x] Live co-editing over `/api/v1/collab/{page_id}` with Yjs (`yrs` on the server, y-websocket protocol), stored as a Yjs document with debounced saves
- [x] Presence: who is on the page, with named, colored cursors
- [x] HTML snapshot for reading, search (FTS) and agents; page title and description in search results
- [x] Version history: automatic snapshots while editing, list, view and restore
- [x] Web: Pages tab, page list, collaborative editor, presence avatars, version panel
- [x] Tests (permissions, sync of two clients through the server, persistence across restarts, versions) and e2e with two browsers

**Review:** Pages ship with live co-editing: one in-memory Yjs room per open page on the server (`yrs` 0.26, the newest that builds on the machine's Rust 1.93; 0.27+ needs a newer compiler), speaking the y-websocket protocol so the stock `y-websocket` client and Tiptap's Collaboration and CollaborationCaret extensions work unchanged. The server, not the browser, builds the HTML snapshot from the Yjs tree (`pagehtml.rs`). An earlier draft let clients send it, but that could lag behind the document when someone left right after typing. Each save (at most every 2 s, and when the last person leaves) updates the HTML, the search index and a version every ten minutes. Readers connect read-only (their updates are dropped). Sockets need a same-site `Origin` (also added to `/api/v1/ws`). Lock, archive, access changes, delete, membership changes and project settings close the sockets so clients reconnect and are checked again. Following PERMISSIONS, only the owner and project admins edit a page; other contributors read it (Plane's documented matrix). Agents get `list_pages`, `get_page` and `create_page` over MCP, and search covers pages. Lock, archive and delete were planned for S3.2 but shipped here. Tests: 4 Rust integration tests (REST rules, two real WebSocket clients syncing through the server, server-built HTML/search/versions, socket closed on removal), a converter unit test, a page-tree unit test, and a two-browser e2e (live typing both ways, named caret, lock turning the other editor read-only, persistence after reload, save and restore a version, search). Totals: 106 Rust, 21 Vitest, 33 Playwright.

### S3.2 — Pages: organization

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅ Done

- [x] Nested pages: move a page under another and reorder by drag and drop (the tree view and nesting rules shipped in S3.1)
- [x] Lock (read-only for everyone until unlocked), archive/restore, delete (shipped early in S3.1)
- [x] Duplicate a page
- [x] Turn selected text into a work item (link inserted in the page); `WEB-12` references render as live links
- [x] Tests and e2e

**Review:** Pages can be dragged in the tree: dropped on the upper or lower third of another page to reorder, in the middle to nest. Drops into the page's own sub-tree are refused (the client checks and the server answers `400`). A **Move** dialog with a parent picker does the same from the keyboard. **Duplicate** copies the text and the live document into a page you own, right after the original. **Create work item** in the editor toolbar turns the selected text into a work item and puts its key before the text. Keys of the workspace's projects (`WEB-12`, not `UTF-8`) are decorated in the editor, never inside code. Hovering one shows a card with the current title and state, fetched when shown; the card's **Open** link or Ctrl/⌘-click goes to the item. `by-key` now returns title and state for the card. A test exposed a race: the server committed a page's HTML before updating the search index. Both now commit in one transaction. The e2e also found that leaving a page within 0.8 s of typing its title lost the change; the pending title is now saved when the view closes. Tests: duplicate/move/reference API test, move-planning and key-pattern unit tests, two e2e (drag to nest, Move dialog, Duplicate; selection → work item → live card → open). Totals: 107 Rust, 26 Vitest, 35 Playwright.

### S3.3 — Intake

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅ Done

- [x] Intake submissions as work items in triage (pending, accepted, declined, duplicate, snoozed)
- [x] Guests and commenters submit and see only their own; admins triage (accept into the project, decline with a reason, mark duplicate of a key, snooze)
- [x] Optional public intake form per project (link with a token, no login, rate-limited, email for follow-up)
- [x] Web: Intake tab with queue and triage panel; notifications to the submitter
- [x] Tests and e2e

**Review:** A request is a work item with `is_draft = 2` (0 normal, 1 draft, 2 in intake) plus an `intake_items` row (migration 0014). Every list, count, cycle/module progress, search index and key lookup already kept only `is_draft = 0`, so requests stay out of the project with no extra filters, and accepting one is a single flag change plus indexing. A deviation from the task line, following PERMISSIONS (Plane): only guests are limited to their own requests; commenters and up see the whole queue and discuss a request in its comments. Contributors cannot change requests through the regular work item API; only admins can, and submitters through intake. Triage writes activity, a *duplicate of* relation for duplicates, and notifications (`updated`/`intake`, so no schema change to the notification kinds). The inbox opens requests in Intake and accepted ones as work items. Guests get no inbox notifications (they cannot read the project); they see the outcome under *Closed*. The public form uses a random link that changes whenever it is turned on again. Submissions come from a per-workspace "Intake form" bot that is not a member. The form has a hidden honeypot field, is rate-limited (5 per address per 10 min, 60 per form per hour), turns text into escaped HTML, and emails the submitter on accept/decline/duplicate when SMTP is set up. Tests: 2 Rust integration tests (visibility, edit rules, triage incl. decline → reopen → accept → searchable, duplicate by key, snooze, withdraw, toggle off; public form incl. honeypot, validation, rate limit, link rotation), an edit-rule unit test, 2 e2e (guest asks → admin accepts → guest sees outcome; public form → decline with reason). Totals: 109 Rust, 27 Vitest, 38 Playwright.

### S3.4 — Analytics

**Dates:** 2026-11-20 → 2026-11-20 · **Status:** ✅ Done

- [x] Aggregation API: counts and estimate points by state group, priority, assignee, label, cycle, module; created vs. completed per week
- [x] Project and workspace dashboards (stat tiles, bar charts, trend line) following the dataviz rules, with table views
- [x] Custom analytics: choose dimension, measure and segment
- [x] Tests and e2e

**Review:** One SQL breakdown handles every dimension pair: per item, `DISTINCT` (item, x, segment) rows, so an item with two labels counts under each label but once in the total. Ordered dimensions (state group, priority, state, cycle) keep their natural order; others sort largest first with "none" last. More than 30 rows or 7 segments fold into "Other". Points count only in projects with point estimates. Only projects the caller may read count, and guests get none. Workspace analytics (sidebar) has a project filter; each project has an Analytics tab. Charts follow the dataviz skill: forms chosen first (tiles for single numbers, ranked or natural-order bars, a two-series line, stacked bars for a segment). Palette: the reference eight hues, validated against our light (`#ffffff`) and dark (`#121417`) surfaces, all checks pass. Light slots 3–5 are below 3:1, so every chart carries a legend, a hover/focus tooltip and a table view. Ordered values keep fixed color slots (Urgent never changes color). The screenshot review led to three fixes: a 2.5 tick on a count axis, SVG text shrinking with a narrow card (the chart now draws at its measured pixel width), and priorities in the legend sorted by size instead of severity. Tests: an analytics API test (tiles, trend, ordering, points by label × priority, visibility per role, guests, bad input) and 2 e2e (overview tiles/tooltip/table/keyboard; custom points × segment in dark mode). Totals: 110 Rust, 27 Vitest, 41 Playwright.

### S3.5 — Import, export, public boards, webhooks, GitHub → web v1

**Dates:** 2026-11-20 → 2026-11-21 · **Status:** ✅ Done

- [x] Export a project's work items as CSV and JSON; import from CSV with a preview and column mapping
- [x] Public boards: a read-only link to a project's list/kanban, revocable, no login
- [x] Webhooks per workspace (events, signed with HMAC-SHA256, retries, delivery log)
- [x] GitHub: a webhook receiver that links pull requests and commits mentioning `WEB-12` to the work item
- [x] Final pass for web v1: docs, README, full test run, owner review list

**Review:**
- **Export** writes CSV (UTF-8 BOM, formula cells defused, descriptions as readable text) or JSON.
- **Import** previews the file, guesses the column mapping from header names (comma or semicolon), matches states, labels and people by name or email, and reports skipped rows and notes by physical line (quoted cells can span lines).
- **Public boards** (`/p/<token>`, migration 0015) show open items with states, priorities, labels, dates and descriptions. People, comments and archived items stay private, and a new link replaces the old one.
- **Webhooks** hang off the existing event bus. Events now carry an `action`, so names read `work_item.created`/`intake.accepted`; the middleware also reads the new item's id from the response of a create, so creation payloads include the item. Deliveries are queued in SQLite, signed with HMAC-SHA256 (own implementation, checked against RFC 4231), retried after 1 min/5 min/30 min/2 h/6 h, logged for 30 days, and can be resent; secrets are shown once.
- **GitHub**: the receiver checks `X-Hub-Signature-256` in constant time. It links pull requests (title, description, branch) and commits that mention keys, and renames the link as the PR changes. With "Fixes/Closes/Resolves WEB-12" a merge completes the item; it acts as a "GitHub" bot and publishes events so screens and webhooks follow.
- **Performance** at 100k items (perf test extended): analytics overview 270 ms, label × priority 450 ms, lists ≤ 54 ms.
- **Tests:** import/export, public board, webhooks (signature, filtering, payload, log, retry schedule, rotate) and GitHub integration tests; HMAC, key-finding and text-conversion unit tests; 5 e2e (CSV import and export, published board seen by a visitor, webhook test delivery verified with Node's crypto, signed GitHub pull request completing an item).
- **Totals:** 117 Rust (+1 ignored perf), 27 Vitest, 46 Playwright.

### S3.6 — Familiar UI: Plane's look and behaviour, our own code

**Dates:** 2026-11-21 → 2026-11-21 · **Status:** ✅ · **Why:** the owner wants to feel at home coming from Plane (SPEC §2 updated: look and behaviour followed, nothing copied). Method: Plane's source was read only to write a prose spec of its patterns and values; the code here is written from that spec.

- [x] Research: prose spec of Plane's tokens, shell, headers, dropdowns, icons, shortcuts, layouts, peek/detail, other pages (no code copied)
- [x] Tokens: Plane-like neutral scale, primary blue, type sizes, radii, shadows, light and dark; icons from `lucide-react` (ISC)
- [x] Shell: sidebar with workspace switcher, new work item + search, nav with icons, projects with expandable sub-navigation and a ⋯ menu, user menu, collapse
- [x] Page header: breadcrumbs and an action cluster, used by every page
- [x] Work items: layout switcher, Filters and Display dropdowns, applied-filter chips, primary "Add work item"
- [x] List and board: group headers, row and card anatomy, property chips, state and priority icons redrawn
- [x] Spreadsheet, calendar, timeline restyled to match
- [x] Peek overview and work item page with a properties sidebar; create modal
- [x] Other pages (projects, cycles, modules, pages, intake, inbox, analytics, settings) on the new header and patterns
- [x] Shortcuts as in Plane, DESIGN.md rewritten, tests and e2e updated, screenshots reviewed in light and dark

**Review:** Two research passes read Plane's repository (main, 2026-11-20) and wrote prose specs of its patterns and values; colour, type, radius and shadow values were read from Plane's published token package. The UI was then rewritten from those notes, so nothing of Plane's code, CSS, icon artwork, text or logo is in the repository (SPEC §2 and DESIGN.md updated). What changed:
- **Look:** Inter and IBM Plex Mono (bundled, OFL), Plane's neutral scale and its current blue (`#006399` / `#2893cc`), 13 px body, 6 px control radius, 24/28/32 px control heights, its shadows, in light and dark (the chart palette re-validated on the new dark surface).
- **Frame:** a top bar on the canvas (workspace switcher with Settings and Invite, a search that opens the palette, inbox with an unread dot, help, user menu), then a rounded panel with a 250 px sidebar (New work item, Home, Your work, Drafts, Workspace › Projects and Analytics, expandable projects with their sections and a ⋯ menu, version in the footer; `Ctrl/⌘B` hides it, an overlay on phones) and 48 px page headers with breadcrumbs and a project switcher.
- **Work items:** the header carries views, search, the five-layout switcher, Filters (a row of joined filter pills with "Add filter"), Display (property pills, group, order, options), Analytics and "Add work item"; list rows and board cards follow Plane's anatomy and chip order, empty properties shrink to icon squares, columns collapse, group "+" opens the create modal with the group's value, rows and cards have a ⋯ menu.
- **Detail:** the peek is a side panel over the content; the page has the five main properties as fields under the title and a slim properties panel with who and when; the create modal has a project picker (also from the sidebar and `N` then `I` on any page), chips with placeholders, "create more", Discard, Save as draft, Create.
- **Elsewhere:** a home page with a greeting, counts, your open work and your projects; project cards with a colour band and logo tile; settings pages with a left menu; segmented controls; the palette at 672 px with icons and key caps; Plane's `G`/`N` key sequences.
Found while testing: the title box in the detail view collapsed to 6 px inside the scrolling column (flex items shrink to their padding when they hide overflow). Emoji and text glyphs that served as icons were replaced with lucide icons throughout. Tests: 27 Vitest, 46 Playwright (updated for the new navigation: workspace menu, project ⋯ menu, filter row) plus a screenshot spec (`NEXTPLANE_SHOTS`) for design review; release binary 20.6 MB.

### S3.7 — Plane Community Edition parity

**Dates:** 2026-11-21 → 2026-11-21 · **Status:** ✅ · **Why:** a comparison with Plane's open-source edition (source of 2026-11-20) found eight things it has that NextPlane lacked; the owner asked for all of them. Same method as S3.6: Plane's source is read only to write a prose spec of its behaviour, and the code is our own.

- [x] Research: prose spec of the eight features' behaviour (no code copied)
- [x] Project archives: archived work items with restore and delete, next to archived cycles and modules
- [x] Automations: auto-archive closed work items and auto-close inactive ones after N months, run by the server
- [x] Workspace views: all work items across projects (built-in tabs and custom views), with filters and display
- [x] Home: quick links, recents and stickies
- [x] Member profile pages: summary, statistics, work lists and activity
- [x] Sign in with GitHub, Google, GitLab and Gitea, set up by the instance admin
- [x] Instance AI settings and a writing assistant in the editor; Unsplash covers for projects
- [x] Workspace exports (several projects, CSV/JSON/XLSX) and comments and reactions on public boards
- [x] Docs, tests and e2e for all of the above; screenshots reviewed in light and dark

**Review:** A research pass read Plane's repository and wrote a prose spec of the eight features (rules, defaults, who may do what); the code was written from it. Migrations 0016–0022. What shipped:
- **Archives** (project ⋯ menu, palette, `/archives`): work items in a list with the usual filters and display, cycles and modules with search; restore and delete from row menus, toasts with a link back. As in Plane, only completed or cancelled work items are archived (`400` otherwise), cycles once completed, modules once completed or cancelled; archived ones are read-only (`409`) until restored, and only contributors and admins see the archive. The menus show the archive entry disabled with the reason.
- **Automations** (project settings): auto-archive closed items and auto-close stale ones after 1–12 months (presets and a custom value), into a cancelled state of the project. A daily pass from 01:00 UTC acts as the workspace's `NextPlane` service account, skips items in running cycles or open-ended modules, and is published like any change (activity, notifications, realtime, webhooks). Activity now carries the actor's name, so service accounts show by name.
- **Workspace views** (`/views`, sidebar, `G` `V`): All work items, Assigned, Created, Subscribed and custom views (shared by default; owner edits, owner or admin deletes; guests see their own) as a spreadsheet across projects, each row edited with its own project's states, labels and people; filters by project and state group. Only projects you are a member of count, as in Plane.
- **Home**: greeting, a "get started" checklist, and quick links, recents (the last 20 things you opened) and stickies (rich text with checklists, eight colours) in your own order (Manage widgets). A stickies page (`G` `N`). All personal: no realtime events or webhooks.
- **Profiles** (`/profile/:id`, from member names; "Your work" is your own, as in Plane): overview and workload tiles, assigned work by priority (chart with a table view), assigned/created/subscribed lists, activity with a one-day CSV download, and a card with per-project completion. Numbers count only the projects the viewer may read.
- **Sign-in providers** (instance admin): Google, GitLab and Gitea over OpenID Connect, GitHub over OAuth with the primary verified email and an optional organization; callback URLs to copy; secrets never sent back. Password sign-in can be turned off (instance admins keep theirs as a way back in); turning off the last way in is refused by the server.
- **AI and Unsplash** (instance admin): a writing assistant on any OpenAI-compatible API ("Draft from title" and "Ask AI" in the create dialog), 60 requests an hour per person, off until a key is saved; Unsplash photos as project covers, credited to their photographer and reported to Unsplash as its API guidelines ask (Plane does neither), next to uploaded covers.
- **Exports** (workspace settings): CSV, JSON or XLSX of the projects you work in, one file (XLSX: a sheet per project, written by our own small ZIP/SpreadsheetML writer, no new dependency), with a history to download again. Nothing is stored on the server.
- **Public boards**: comments, reactions and votes, each switched on per project; visitors read freely and sign in to take part; only comments marked public show, and the team can make an app comment public or take it off.
- **Shared pieces**: toasts on every page; the archive and delete actions shared by row menus and the detail view.

Found while testing: the description editor saved unchanged text whenever it switched between editable and read-only (Tiptap's `setEditable` emits an update), which also hit archived items with `409`; the AI popover was a form inside the create form, so Generate submitted the work item. Both fixed. The XLSX output was checked as ZIP and XML with Python; opening it in a spreadsheet program was not possible here. Tests: 132 Rust (+1 ignored perf), 27 Vitest, 54 Playwright (+3 screenshot specs), screenshots of the new screens in light and dark in `../.generate`.

### Web v1 — owner review

Phases 0–3.6 are done (S3.7 not released yet); v0.1.1 is released. These need the owner:

1. **Declare web v1 final** (or list changes). That starts phase 4 (desktop, Android).
2. **Git:** ✅ `azamrizki/nextplane`, public since 2026-11-21 (history checked first: no secrets, no `.env`, keys or databases, commits carry no email); `v0.1.0` released.
3. **Lint:** ✅ `rustfmt` and `clippy` installed from the distro (1.93); `cargo fmt --check` and `cargo clippy --all-targets -D warnings` pass.
4. **Rust version:** `yrs` is pinned to 0.26 because 0.27+ needs a newer compiler than 1.93; with rustup the pin can go.
5. **Behaviour to confirm** (the open ones follow Plane's documented rules today):
   - ✅ Pages: every contributor now co-edits any project page (owner's yes on 2026-11-21, PERMISSIONS D7). Lock, archive, delete and access stay with the page's owner or an admin.
   - Commenters and up see the whole intake queue; guests see only their own requests and get no inbox notification for the outcome (they see it under *Closed*).
   - Public boards show descriptions.
   - Webhooks may call addresses on your own network (only workspace admins can add them).
   - GitHub "close on merge" is on by default.
6. **Try it by hand** with `bin/nextplane serve`:
   1. Set up the instance and a workspace with a project.
   2. Edit one page from two browsers at once.
   3. Send a request through the public intake form and triage it.
   4. Open analytics.
   5. Import a CSV, then publish the board.
   6. Add a webhook (for example to a request bin) and connect a test GitHub repository.
7. **Release check (2026-11-21):** the release workflow had two runners GitHub retired in November 2026 (`macos-13`, `macos-14`) and would have failed on the tag; it now uses `macos-15-intel` and `macos-15`, newer actions (Node 24), and a manual trial mode that builds every target, checks the Linux builds are static, starts each binary on a fresh data folder and fetches its health, version and web page, without publishing. The crate version was still `0.1.0-dev`, so a `v0.1.0` binary would have kept asking `nextplane update` for itself; it is `0.1.0` now and a tag that does not match the crate version stops the release. `install.sh` (also piped into `sh`) and `nextplane update` were run against a local copy of a release: install, wrong checksum (nothing installed or changed), `--check`, update with `nextplane.old` kept. `update` now restarts the service only if the service runs the binary it replaced. Once the repository was public, both were run against GitHub with no login: `curl … install.sh | sh` installed the v0.1.0 release, `nextplane update --check` found v0.1.0 and `update` answered "already up to date".

## Phase 4 — Clients

⏸ On hold until the owner declares web v1 stable and final.

- Desktop: Tauri v2, reusing `np-client`
- Android: Kotlin + Jetpack Compose

---

## Changelog of this file

| Date | Change |
|---|---|
| 2026-11-20 | Created: phase overview, phase 0 and phase 1 sprints with task checklists, phase 2–4 outline. |
| 2026-11-20 | S0.1 done (10/10); S0.2 gains a task for the sqlx query-check decision. |
| 2026-11-20 | S0.2 done (12/12), phase 0 done; S1.1 started with the carried fmt/clippy task. |
| 2026-11-21 | S3.7 done: archives, automations, workspace views, home widgets, profiles, sign-in providers, AI and Unsplash, exports and board interactions. |
| 2026-11-21 | S3.7 started: Plane Community Edition parity (eight features). |
| 2026-11-21 | v0.1.1 released (patch, owner's go-ahead): the S3.6 UI; the Linux amd64 download matched `SHA256SUMS`, reports 0.1.1 and serves the new UI. |
| 2026-11-21 | S3.6 done: the web app follows Plane's look and behaviour, rebuilt in our own code. |
| 2026-11-21 | Repository made public; installer and updater checked against GitHub. |
| 2026-11-21 | v0.1.0 tagged and released; phase 1 done (60/60). |
| 2026-11-21 | Release check: retired macOS runners replaced, trial mode, version 0.1.0, installer and updater tested against a local release. |
| 2026-11-21 | Contributors co-edit every project page (D7, owner's decision). |
| 2026-11-21 | fmt/clippy pass done; pushed to `azamrizki/nextplane`. |
| 2026-11-21 | S3.5 done; phase 3 done; web v1 feature-complete, owner review list added. |
| 2026-11-20 | S3.4 done (analytics dashboards and custom charts). |
| 2026-11-20 | S3.3 done (intake with triage and a public form). |
| 2026-11-20 | S3.2 done (page organization, live work item references). |
| 2026-11-20 | S3.1 done (collaborative pages). |
| 2026-11-20 | Phase 3 broken into tasks; S3.1 started. |
| 2026-11-20 | S2.5 done; phase 2 done. |
| 2026-11-20 | S2.4 done. |
| 2026-11-20 | S2.3 done. |
| 2026-11-20 | S2.2 done. |
| 2026-11-20 | S2.1 done. |
| 2026-11-20 | Phase 2 broken into tasks; S2.1 started. |
| 2026-11-20 | S1.6: 10/12 done; fmt/clippy and the v0.1.0 tag wait for the owner. |
| 2026-11-20 | S1.5 done. |
| 2026-11-20 | S1.4 done. |
| 2026-11-20 | S1.3 done. |
| 2026-11-20 | S1.2 done. |
| 2026-11-20 | S1.1 done; role tasks in S1.2/S1.3 follow Plane's current roles (contributor/commenter); fmt/clippy moved to S1.6, avatar upload to S1.4. |
