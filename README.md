# NextPlane

**Self-hosted project management in one binary.**

Work items, kanban and list boards, comments, files, real-time updates and an API for people and AI agents — in a single `nextplane` binary with SQLite inside. No PostgreSQL, Redis, queue or object store to run.

NextPlane takes [Plane](https://github.com/makeplane/plane) as its functional reference and is written from scratch in Rust and React (see [docs/SPEC.md](docs/SPEC.md) §2). License: MIT.

> **Status: web v1 is feature-complete** (phases 0–3 of [docs/SPRINT.md](docs/SPRINT.md)) and waiting for the owner's review. Desktop and Android apps (phase 4) start once web v1 is declared final.

## Quick start

```sh
curl -fsSL https://raw.githubusercontent.com/azamrizki/nextplane/main/install.sh | sh
nextplane logs        # shows the one-time setup link (valid 30 minutes)
```

The installer checks the release's SHA-256, puts `nextplane` in `~/.local/bin` and starts a user service (systemd on Linux, a LaunchAgent on macOS). Open the setup link, create the admin account, then your first workspace. Lost the link? `nextplane setup-link`.

Options: `sh -s -- --version=v0.1.0` for a given release, `--no-service` to only install the binary. Mirrors: `NEXTPLANE_DOWNLOAD_BASE` (a folder with the archives and `SHA256SUMS`) for the installer, `NEXTPLANE_UPDATE_API` and `NEXTPLANE_UPDATE_REPO` for `nextplane update`.

| Default | Value |
|---|---|
| Address | `http://127.0.0.1:12313` (plain HTTP on this machine) |
| Data | `~/.nextplane` (`--data` or `NEXTPLANE_HOME` to change): `nextplane.db`, `uploads/`, `backups/`, `config.toml` |
| Other devices | Put nginx or Caddy with HTTPS in front ([deploy/](deploy/)), or use a private network such as Tailscale. Set `public_url` in `config.toml`. |

### Build from source

Needs Rust (stable) and Node.js 22+ (CI uses 24).

```sh
make build              # web app + release binary → bin/nextplane
./bin/nextplane serve   # prints the setup link
make dev                # server + Vite with hot reload on http://localhost:15173
make test               # Rust and web tests
```

## What it does

- **Workspaces** with owners, admins, members and guests; invite links (and email when SMTP is set); open, invite-only or closed sign-up; 30-day trash.
- **Projects**, public or private, with Plane's project roles (admin, contributor, commenter, guest), custom states per group, nested labels, point or T-shirt estimates, archive.
- **Work items** with keys like `WEB-12`, rich-text descriptions (task lists, code, images), state, priority, assignees, labels, dates, estimates, parents and sub-items, relations (blocks, relates to, duplicate of) and a full activity history.
- **List, kanban, spreadsheet, calendar and timeline** layouts with grouping, ordering, filters, display options, bulk edit and drag and drop — fast with 100,000 work items in a project. **Saved views**, personal or shared.
- **Cycles** (sprints) with progress and burn-down, moving unfinished work on; **modules** for features or areas.
- **Pages**: documents written together in real time, with named cursors, presence, nesting, lock, private pages and version history.
- **Intake**: requests from anyone in the project, or from people without an account through a public form, triaged by admins (accept, decline, duplicate, snooze).
- **Analytics** for a project or the whole workspace: open, completed and overdue work, a weekly created/completed trend, breakdowns by state, priority and assignee, and custom charts by any dimension and segment (count or estimate points), each with a table view.
- **Import and export**: CSV import with a preview and column mapping; CSV and JSON export.
- **Public boards**: a read-only link to a project's board for people without an account.
- **Webhooks** (signed, retried, with a delivery log) and **GitHub**: pull requests and commits that mention `WEB-12` are linked, and "Fixes WEB-12" completes the item on merge.
- **Inbox** with notifications for what you follow, @mentions and assignments, snooze, and an optional email digest; **My work** across projects; **drafts**.
- **Collaboration**: comments with @mentions, reactions, attachments, links, subscriptions, and live updates in every open browser.
- **Search** across titles, descriptions and comments, and a command palette (`Ctrl/⌘K`).
- **API and agents**: personal API tokens, bot accounts, and an **MCP server** (`/mcp`) so agents such as NextBoard's or Claude Code can read, create and update work — see [docs/API.md](docs/API.md).
- **Single sign-on** with any OpenID Connect provider (Google, Microsoft, Keycloak, Authentik, …).
- **English and Indonesian**, light and dark themes.

Who may do what is listed in [docs/PERMISSIONS.md](docs/PERMISSIONS.md).

## Operating it

| Command | What it does |
|---|---|
| `nextplane serve` | Run the server in the foreground. |
| `nextplane install` / `uninstall` | Add or remove the user service (`--dry-run` shows the steps). The data folder is never removed. |
| `nextplane status` / `logs [-f]` | Is it running; the service log. |
| `nextplane backup <file.tar.gz>` | Database, uploads and `config.toml` in one archive, safe while running. The archive holds your secrets; keep it private. |
| `nextplane restore <file.tar.gz>` | Only while stopped. Asks for confirmation, backs up the current state first and moves the replaced files to `backups/replaced-*`. |
| `nextplane update [--version vX.Y.Z] [--check]` | Download a release, verify its checksum, swap the binary (the previous one stays as `nextplane.old`) and restart the service if it runs this binary. |
| `nextplane admin reset-password <email>` | Print a one-time reset link. |

The running server also writes a daily copy of the database to `backups/daily-YYYY-MM-DD.db` (the newest 14 are kept) and backs up the database before every schema migration.

### config.toml

```toml
listen = "127.0.0.1:12313"
log_format = "pretty"                        # or "json"
public_url = "https://plane.example.com"     # the address people use

[smtp]                                       # optional: invites and resets by email
host = "smtp.example.com"
port = 587
username = "…"
password = "…"
from = "NextPlane <noreply@example.com>"
security = "starttls"                        # starttls | tls | none

[oidc]                                       # optional single sign-on
issuer = "https://accounts.google.com"
client_id = "…"
client_secret = "…"
name = "Google"                              # "Sign in with Google"
# register the redirect URI <public_url>/api/v1/auth/oidc/callback with the provider
```

## Documentation

| Document | Contents |
|---|---|
| [docs/SPEC.md](docs/SPEC.md) | Goals, decisions, architecture, data model, phases |
| [docs/SPRINT.md](docs/SPRINT.md) | Progress per phase and sprint |
| [docs/PERMISSIONS.md](docs/PERMISSIONS.md) | Roles and the permission matrix |
| [docs/API.md](docs/API.md) | HTTP API, tokens, realtime |
| [docs/DESIGN.md](docs/DESIGN.md) | Design tokens and UI rules |
