# NextPlane HTTP API

Version 1, under `/api/v1`. The web app uses exactly this API; anything it does, a script or agent can do. The list below mirrors `crates/np-api/src/lib.rs`; the role rules are in [PERMISSIONS.md](PERMISSIONS.md).

## Conventions

| Topic | Rule |
|---|---|
| Format | JSON in and out (`Content-Type: application/json`), except file uploads (multipart field `file`) and file downloads. |
| IDs | UUIDv7 strings. Work items also have a key: `<project identifier>-<sequence>`, e.g. `WEB-12`. |
| Time | Timestamps are UTC, `2026-11-20T08:39:28.057676Z` (fixed width, sortable). Dates are `YYYY-MM-DD`. |
| Errors | `{"error": {"code": "...", "message": "..."}}` with the matching status. Codes: `bad_request` 400, `unauthorized` 401, `forbidden` 403, `not_found` 404, `method_not_allowed` 405, `conflict` 409, `rate_limited` 429, `internal` 500. A body, query or path that does not parse is a `bad_request` that says why. |
| Not found vs. forbidden | Things you may not see answer `404` (workspaces you are not in, private projects, files). `403` means you can see it but your role does not allow the action. |
| PATCH | Missing fields stay; `null` clears a field that can be empty (dates, estimate, parent, icon…). |
| Paging | Work item lists return at most `limit` items (default 1000, max 5000) in board order. If there are more, the `X-Next-Cursor` response header holds the cursor for `?cursor=`. Other lists are bounded in size and return everything. |
| Realtime | See [Realtime](#realtime). Send `X-Client-Id: <any string>` on your requests to recognise your own events. |

## Authentication

- **Browser session.** `POST /auth/login` sets an `HttpOnly`, `SameSite=Lax` cookie (`Secure` behind HTTPS) valid for 30 days of inactivity. State-changing requests with an `Origin` header must come from the same host.
- **API tokens.** `Authorization: Bearer np_…`. Personal tokens act as their user; `read` tokens may only `GET`. Bots (below) are workspace members that exist only through tokens, for agents such as NextBoard's.
- **Rate limits.** Login (per IP and per email), setup, signup, password reset, invite creation and opening invite links are limited; the answer is `429 rate_limited`.

## Endpoints

### Instance, setup, account

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/health` | anyone | `{status, schema_version}` |
| GET | `/version` | anyone | `{version, commit, build_date}` |
| GET | `/instance` | anyone | `{name, setup_required, signup_policy, email_enabled}` |
| POST | `/setup/verify` | anyone | `{token}` — checks a setup link |
| POST | `/setup` | anyone with the link | `{token, instance_name, name, email, password}` — creates the instance admin, logs in |
| POST | `/auth/login` | anyone | `{email, password}` → user, sets the cookie |
| POST | `/auth/logout` | session | ends the session (204) |
| POST | `/auth/signup` | anyone, policy `open` | `{email, name, password}` → user (201) |
| POST | `/auth/forgot` | anyone | `{email}` — emails a reset link when SMTP is set; always 204 |
| GET / POST | `/auth/reset/{token}` | link holder | `GET` → `{email}`; `POST {password}` sets it, ends all sessions, logs in |
| GET / PATCH | `/me` | user | the user; `PATCH {name?, avatar?}` |
| POST | `/me/password` | user | `{current, new}` — other sessions end (204) |
| POST | `/me/avatar` | user | multipart image (PNG/JPEG/GIF/WebP, 2 MB) → user |
| GET / POST | `/me/tokens` | user (not bots) | list; `POST {name, scope: read|write, expires_in_days?}` → token metadata + `token` (shown once) |
| DELETE | `/me/tokens/{id}` | user | revoke |

### Workspaces

| Method | Path | Who | What |
|---|---|---|---|
| GET / POST | `/workspaces` | user | my workspaces with `role`; `POST {name, slug?}` (slug from the name if missing) |
| GET | `/workspaces/slug-check?slug=` | user | `{available, reason?}` |
| GET | `/workspaces/trash` | user | workspaces I own that are in the trash |
| GET / PATCH / DELETE | `/workspaces/{slug}` | member / admin / owner | `PATCH {name?, slug?, logo?, members_can_create_projects?}`; `DELETE` moves to the trash for 30 days |
| POST | `/workspaces/{slug}/restore` | owner | restore from the trash |
| POST | `/workspaces/{slug}/logo` | admin | multipart image → `{logo}` |
| GET | `/workspaces/{slug}/members` | member+ | members with roles |
| PATCH / DELETE | `/workspaces/{slug}/members/{user_id}` | per matrix | `{role}`; remove (last owner and sole project admins are protected) |
| POST | `/workspaces/{slug}/leave` | member | leave |
| GET | `/workspaces/{slug}/members/{user_id}/profile` | member | a member's profile, counted only in the live projects **you** are a member of and may read: `user{id, name, email, avatar, role, joined_at}` (no `email` for guests looking at someone else), `created`, `assigned`, `subscribed`, `pending` (assigned and open), `completed`, `by_state_group` and `by_priority` of assigned items (`[{key, count}]`), and `projects[{id, name, identifier, icon, cover_color, created, assigned, pending, completed}]` (left out for guests) |
| GET | `/workspaces/{slug}/members/{user_id}/activity?cursor=&limit=&date=&format=` | member (not guest) | what they did to work items in the projects you may read, newest first, each with `identifier`, `title`, `sequence`; pages of `limit` (default 100) with `X-Next-Cursor`; `date=YYYY-MM-DD&format=csv` downloads one UTC day (at most 10,000 rows) |
| GET / POST | `/workspaces/{slug}/invites` | admin | pending invites; `POST {invites: [{email, role}]}` → per email `{link?, emailed, error?}` |
| DELETE | `/workspaces/{slug}/invites/{id}` | admin | revoke |
| GET / POST | `/workspaces/{slug}/bots` | admin | bots with their tokens; `POST {name, role?, scope?}` → `{bot, token}` |
| DELETE | `/workspaces/{slug}/bots/{id}` | admin | remove the bot and revoke its tokens |
| POST | `/workspaces/{slug}/bots/{id}/token` | admin | revoke the bot's tokens and issue a new one → `{token}` |
| GET | `/workspaces/{slug}/search?q=&limit=` | member | `{projects, work_items}` — full-text over titles, descriptions and comments in projects you can read; `WEB-12` resolves directly |
| GET | `/workspaces/{slug}/work-items/by-key/{KEY-n}` | reader | `{project_id, id, key, title, state{name, color, group}, archived}` (drafts are not found) |

### Invites (public links)

| Method | Path | What |
|---|---|---|
| GET | `/invites/{token}` | workspace, email, role, `account_exists`, `signup_allowed` |
| POST | `/invites/{token}/accept` | logged in as the invited email → `{workspace_slug}` |
| POST | `/invites/{token}/signup` | `{name, password}` — creates the invited account, logs in |

### Projects

Base: `/workspaces/{slug}/projects`.

| Method | Path | Who | What |
|---|---|---|---|
| GET / POST | `` | member / creator | projects you can see (`?archived=true` for archived), each with `my_role`, `is_member`, `member_count`; `POST {name, identifier?, description?, icon?, cover_color?, lead_id?, is_private?}` |
| GET / PATCH / DELETE | `/{id}` | reader / admin / admin | `PATCH` any of the create fields plus `default_assignee_id`, `estimate_type` (`none|points|tshirt`), `estimate_values`, `feature_cycles|modules|pages|intake`, `cover_image` (an uploaded file's `/api/v1/files/{id}` or an Unsplash photo's `https://images.unsplash.com/…`; `null` removes it) with `cover_credit{name, url}` (required for Unsplash photos, shown with the photo) and `unsplash_download` (the photo's `download_location`: the server tells Unsplash it was used, as Unsplash's API guidelines ask), and the automations `archive_in`, `close_in` (months 0–12, 0 = off) and `close_state_id` (a cancelled state of the project; the first one when auto-close is turned on without it, cleared when it is turned off). Once a day from 01:00 UTC the server archives items closed (completed or cancelled) for `archive_in` × 30 days and moves open items unchanged for `close_in` × 30 days to `close_state_id`, skipping items in a cycle that has not ended or a module whose target date has not passed; the changes are made by the workspace's `NextPlane` service account (activity, notifications, realtime events and webhooks follow) |
| POST | `/{id}/archive`, `/{id}/unarchive` | admin | archived projects are read-only |
| POST | `/{id}/join`, `/{id}/leave` | member | join a public project / leave |
| GET / POST | `/{id}/members` | member / admin | `POST {user_id, role}` |
| PATCH / DELETE | `/{id}/members/{user_id}` | admin | `{role}`; guest ceiling and last-admin rules apply |
| GET / POST | `/{id}/states` | member / admin | `POST {name, color, group}` |
| PATCH / DELETE | `/{id}/states/{state_id}` | admin | `PATCH {name?, color?, group?, sort_order?, is_default?}` → all states; delete refuses the default state and states in use |
| GET / POST | `/{id}/labels` | member / admin | `POST {name, color, parent_id?}` (one level of nesting) |
| PATCH / DELETE | `/{id}/labels/{label_id}` | admin | `PATCH` → all labels |
| POST | `/{id}/uploads` | commenter+ | multipart image for descriptions and comments → `{id, url}` |

### Work items

Base: `/workspaces/{slug}/projects/{id}/work-items`.

| Method | Path | Who | What |
|---|---|---|---|
| GET | `` | commenter+ (`archived=true`: contributor+) | filters: `state`, `state_group`, `priority`, `assignee`, `unassigned=true`, `label`, `created_by` (comma-separated), `parent`, `archived=true`, `due_before`, `due_after`, `q` (title or key), `limit`, `cursor` |
| POST | `` | contributor+ | `{title, description_json?, description_html?, state_id?, priority?, start_date?, due_date?, estimate?, parent_id?, assignee_ids?, label_ids?}` (201) |
| PATCH | `` | contributor+ | bulk: `{ids: [...], state_id?, priority?, assignee_ids?, label_ids?, start_date?, due_date?, estimate?}` |
| GET / PATCH / DELETE | `/{wid}` | commenter+ / contributor+ / owner of the item or admin | `PATCH` any create field plus `sort_order` and `expected_updated_at` (see below) |
| POST | `/{wid}/archive`, `/{wid}/unarchive` | contributor+ | only items in a completed or cancelled state are archived (`400` otherwise). An archived item is read-only: `PATCH`, also in bulk, answers `409` until it is restored; it can still be deleted |
| GET | `/{wid}/activity` | commenter+ | one row per change: `{actor_id, actor_name, actor_is_bot, verb, field, old_value, new_value, created_at}` |
| GET / POST | `/{wid}/relations` | commenter+ / contributor+ | `POST {kind: blocks|blocked_by|relates_to|duplicate_of, work_item_id}` → all relations from this item's side |
| DELETE | `/{wid}/relations/{rid}` | contributor+ | |
| GET / POST | `/{wid}/comments` | commenter+ | `POST {body_json?, body_html}`; `@mentions` are `<span data-type="mention" data-id="<user id>">` |
| PATCH / DELETE | `/{wid}/comments/{cid}` | the author only | |
| POST | `/{wid}/comments/{cid}/hide`, `/unhide` | admin | |
| POST | `/{wid}/comments/{cid}/reactions` | anyone in the project | `{emoji}` toggles |
| GET / POST | `/{wid}/reactions` | anyone in the project | `{emoji}` toggles |
| GET / POST | `/{wid}/attachments` | commenter+ / contributor+ | multipart `file` (25 MB by default) |
| DELETE | `/{wid}/attachments/{fid}` | uploader or admin | |
| GET / POST | `/{wid}/links` | commenter+ / contributor+ | `POST {url, title?}` (http/https only) |
| DELETE | `/{wid}/links/{lid}` | contributor+ | |
| GET / POST / DELETE | `/{wid}/subscription` | commenter+ | `{subscribed}` |

**Rich text.** `description_html` and comment HTML are sanitized on the server with an allow-list (no scripts, event handlers or `javascript:` links); `description_json` is the editor's document, stored as given.

**Concurrent edits.** Send `expected_updated_at` (the `updated_at` your text was based on) with a title or description change. If someone else changed the title or description after that, the answer is `409 conflict`; your own earlier saves and other people's property changes never conflict.

### Files

| Method | Path | What |
|---|---|---|
| GET | `/files/{fid}` | the bytes, to people who may see what the file belongs to (avatars: any logged-in user; logos: workspace members). Images, PDF and plain text show inline; everything else downloads. Always `X-Content-Type-Options: nosniff` and a sandboxing CSP. |

### Instance admin

| Method | Path | What |
|---|---|---|
| GET / PATCH | `/admin/settings` | `{name, signup_policy: closed|invite|open, email_enabled, public_url}` |
| GET / PATCH | `/admin/auth` | sign-in methods: `{password_enabled, signup_policy, config_sso, providers[{key, name, enabled, configured, client_id, has_secret, host, org, callback_url}]}` for `google`, `github`, `gitlab`, `gitea`. `PATCH {password_enabled?, providers: {<key>: {enabled?, client_id?, client_secret?, host?, org?}}}`: a provider is turned on only once it has a client id and secret (and a host for GitLab and Gitea); an empty `client_secret` keeps the saved one, which is never sent back. Turning off the last way to sign in is `409` |
| GET / PATCH | `/admin/services` | the AI assistant and Unsplash: `{ai{enabled, has_key, model, base_url}, unsplash{enabled}}`. `PATCH {ai?{api_key?, model?, base_url?}, unsplash?{access_key?}}`; an empty key removes it; keys are never sent back. The AI service is any OpenAI-compatible Chat Completions API (default `https://api.openai.com/v1`, model `gpt-4o-mini`) |
| GET | `/admin/users` | all users |
| PATCH | `/admin/users/{id}` | `{is_instance_admin}` (one admin always stays) |
| POST | `/admin/users/{id}/reset-link` | `{link, expires_in_hours}` |
| GET | `/admin/workspaces` | all workspaces with member counts |

### Cycles, modules, views

Base: `/workspaces/{slug}/projects/{id}`. Cycles and modules answer `403` while the project's feature toggle is off.

| Method | Path | Who | What |
|---|---|---|---|
| GET / POST | `/cycles` | reader / contributor+ | `?archived=true` (contributor+); `POST {name, description?, start_date?, end_date?, owner_id?}` — both dates or neither; ranges may not overlap (`409`). Each cycle has `status` (`draft`, `upcoming`, `current`, `completed`) and item counts |
| GET / PATCH / DELETE | `/cycles/{cid}` | reader / contributor+ / creator or admin | delete keeps the items |
| POST | `/cycles/{cid}/archive`, `/unarchive` | contributor+ | only `completed` cycles (end date passed) are archived (`400`); an archived cycle answers `409` to `PATCH` and `DELETE` until restored |
| GET | `/cycles/{cid}/progress` | reader | `{groups[{group, items, points}], total_items, completed_items, total_points, completed_points, burndown[{date, ideal, remaining}]}` |
| POST | `/cycles/{cid}/transfer` | contributor+ | `{to_cycle_id}` moves unfinished items → `{moved}` |
| GET / POST | `/modules` | reader / contributor+ | `?archived=true` (contributor+); `POST {name, description?, status?, lead_id?, start_date?, target_date?, member_ids?}`; status is `backlog`, `planned`, `in_progress`, `paused`, `completed`, `cancelled` |
| GET / PATCH / DELETE | `/modules/{mid}` | reader / contributor+ / creator or admin | |
| POST | `/modules/{mid}/archive`, `/unarchive` | contributor+ | only `completed` or `cancelled` modules are archived (`400`); an archived module answers `409` to `PATCH`, `DELETE` and link changes until restored |
| GET | `/modules/{mid}/progress` | reader | as for cycles (burn-down from start to target date) |
| GET / POST, DELETE | `/modules/{mid}/links`, `/modules/{mid}/links/{lid}` | reader / contributor+ | `POST {url, title?}` |
| GET / POST | `/views` | reader / contributor+ | saved views you may see (shared, or yours); `POST {name, is_shared?, config}` — `config` is the web app's view state (JSON object, 32 KB max) |
| PATCH / DELETE | `/views/{vid}` | owner or admin | only the owner changes `is_shared` |

Work items take `cycle_id` (one) and `module_ids` (many) on create and update, and filter with `?cycle=<id>,none` and `?module=<id>,none`.

### Pages

Base: `/workspaces/{slug}/projects/{id}`. Pages answer `403` while the project's pages toggle is off. Contributors and admins create pages and edit any project page (the owner's choice, PERMISSIONS D7); only the page's owner or an admin locks, archives, deletes or changes access; commenters and guests read. A `private` page is seen only by its owner (others get `404`). A locked or archived page is read-only for everyone.

| Method | Path | Who | What |
|---|---|---|---|
| GET / POST | `/pages` | reader / contributor+ | `?archived=true`; `POST {title?, parent_id?, access?: "project" \| "private"}`. Each page has `can_edit` and `can_manage` for you |
| GET | `/pages/{pid}` | reader | adds `html` (sanitized snapshot) and `has_doc` |
| PATCH | `/pages/{pid}` | editor | `{title?, parent_id?, sort_order?, access?}` (`access` only by the owner; a page cannot move inside itself or its sub-pages, `400`). `html` only sets the first text of a page nobody has opened in the editor yet; afterwards it is `409` |
| POST | `/pages/{pid}/do/{action}` | owner or admin | `lock`, `unlock`, `archive`, `unarchive` |
| POST | `/pages/{pid}/duplicate` | contributor+ | a copy you own, `"<title> (copy)"`, placed right after the original with the same text, parent and access |
| DELETE | `/pages/{pid}` | owner or admin | |
| GET / POST | `/pages/{pid}/versions` | reader / editor | list (newest first, 200 max); `POST` keeps a version of the page as it is now |
| GET | `/pages/{pid}/versions/{vid}` | reader | one version with its `html` |

**Live editing.** `GET /api/v1/collab/{pid}` upgrades to a WebSocket speaking the [y-websocket](https://github.com/yjs/y-websocket) protocol (Yjs sync and awareness, binary frames); the document is Tiptap's ProseMirror tree in the `default` XML fragment. Browsers must send a same-site `Origin`. Readers receive changes but their edits are ignored. The server saves the document at most every two seconds while it changes and when the last person leaves, builds the HTML snapshot from it, updates search, and keeps a version every ten minutes of editing. When rights change (lock, archive, access, delete, membership, project settings) the server closes the sockets; clients reconnect and are checked again.

### Intake

Base: `/workspaces/{slug}/projects/{id}`; `403` while the project's intake toggle is off. A request is a work item kept out of the project (lists, counts, search, keys) until it is accepted. Everyone in the project submits; guests see only their own requests; commenters and up see all and discuss them in the work item's comments; project admins triage.

| Method | Path | Who | What |
|---|---|---|---|
| GET / POST | `/intake` | member | `?view=open` (pending, and snoozed whose time came) `\| snoozed \| closed \| all`; `POST {title, description_html?, priority?}` notifies the project's admins |
| GET | `/intake/count` | member | `{open}` |
| GET / PATCH / DELETE | `/intake/{wid}` | submitter or admin | entry with `status`, `reason`, `duplicate_of`/`duplicate_sequence`, `snoozed_until`, `source` (`app`, `form`), `submitter_name`, `submitter_email` (admins only), `description_html`. Submitters change or withdraw their own while pending or snoozed; admins until it is accepted |
| POST | `/intake/{wid}/{action}` | admin | `accept`; `decline {reason?}`; `duplicate {duplicate_of: "WEB-12"}` (same project; adds a *duplicate of* relation); `snooze {until}`; `reopen`. Accepted is final (`409`). The submitter is notified; form submitters with an email get one when SMTP is set up |
| GET / POST | `/intake-form` | admin | `{enabled, path}`; `POST {enabled}` — turning it on always makes a new link |
| GET / POST | `/api/v1/public/intake/{token}` | anyone | the form's `{workspace, project}`; `POST {title, description?, name?, email?}` → `202`. Five per address per ten minutes, sixty per form per hour |

The regular work item API reads requests (commenters and up) but only project admins may change them there.

### Analytics

Counts cover live work items (not archived, drafts or requests in intake) of the projects you may read; workspace guests get `403`. `?project=<id>` narrows to one project (`404` if you cannot read it).

| Method | Path | What |
|---|---|---|
| GET | `/workspaces/{slug}/analytics/overview?project=` | `{total, open, completed_30d, overdue, points_total, points_open, weeks[{start, created, completed}] (12 weeks, Mondays, UTC), by_state_group, by_priority, by_assignee}` |
| GET | `/workspaces/{slug}/analytics?x=&segment=&measure=&project=` | a breakdown: `x` and optional `segment` are `state_group`, `state`, `priority`, `assignee`, `label`, `cycle`, `module`, `project` (different from each other); `measure` is `count` (default) or `points` (projects with point estimates). Answers `{x[{key, label, color, total}], segments[…], cells[{x, segment, value}], total}`. Key `""` is "none" (unassigned, no label…), `"__other"` the folded tail (more than 30 rows or 7 segments). Ordered dimensions come in their natural order, others largest first. An item with two labels counts under both, once in `total` |

### Import and export

Base: `/workspaces/{slug}/projects/{id}`.

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/export?format=csv\|json` | reader | every work item (archived too, no drafts or intake) with names instead of ids, as a download (`WEB-work-items.csv`). CSV starts with a UTF-8 byte order mark; cells that a spreadsheet would run as formulas get a leading `'`; descriptions are plain text. JSON keeps `description_html` |
| POST | `/import/preview` | contributor+ | the CSV file as the body (≤ 5 MB, ≤ 5,000 rows, comma or semicolon) → `{headers, rows (first 10), total_rows, mapping, fields}`; `mapping` guesses field → column from the header names |
| POST | `/import` | contributor+ | `{csv, mapping: {title: 0, state: 2, …}}` (fields: `title`, `description`, `state`, `priority`, `assignees`, `labels`, `start_date`, `due_date`, `estimate`) → `{created, skipped[{line, message}], warnings[{line, message}]}`. States, labels and people are matched by name or email; values that do not match are left out with a warning; rows without a title are skipped |

### Public boards

| Method | Path | Who | What |
|---|---|---|---|
| GET / POST | `/workspaces/{slug}/projects/{id}/publish` | project admin | `{enabled, path}`; `POST {enabled}` — publishing always makes a new link (`/p/<token>`) |
| GET | `/api/v1/public/boards/{token}` | anyone | `{workspace, project{name, identifier}, states, labels, items[{sequence, title, state_id, priority, start_date, due_date, estimate, label_ids}]}`: open items only (no archived, drafts or intake), 2,000 at most. No people, comments or files |
| GET | `/api/v1/public/boards/{token}/items/{sequence}` | anyone | one item with `description_html` |

### Webhooks

Workspace owners and admins. `POST /workspaces/{slug}/webhooks {url, events}` (`events`: `["*"]` or areas: `work_item`, `comment`, `attachment`, `link`, `relation`, `reaction`, `cycle`, `module`, `view`, `page`, `intake`, `project`, `state`, `label`, `member`; at most 20 webhooks) answers with the `secret`, shown only then and after `POST …/{id}/rotate`. Also `GET` (list), `PATCH /{id} {url?, events?, active?}`, `DELETE /{id}`, `POST /{id}/ping`, `GET /{id}/deliveries` (last 50) and `POST /{id}/deliveries/{did}/redeliver`.

Each change is POSTed as JSON:

```json
{"event": "work_item.updated", "workspace": "acme", "project_id": "…", "work_item_id": "…", "actor_id": "…", "at": "…",
 "work_item": {"id": "…", "key": "WEB-12", "title": "…", "state": {"name": "In Progress", "group": "started"}, "priority": "high",
               "assignee_ids": [], "label_ids": [], "start_date": null, "due_date": null, "in_intake": false, "url": "https://…"},
 "delivery_id": "…"}
```

Headers: `X-NextPlane-Event`, `X-NextPlane-Delivery`, and `X-NextPlane-Signature: sha256=<hex HMAC-SHA256 of the raw body with the secret>` — compare it in constant time before trusting the body. Event names are `<area>.<action>`: `created`, `updated`, `deleted`, or what an action did (`archived`, `accepted`, `declined`, …). A delivery succeeds on any 2xx within 10 seconds; otherwise it is retried after 1 min, 5 min, 30 min, 2 h and 6 h, then marked failed. Logs are kept 30 days. Only workspace admins can add webhooks, and NextPlane will call any http(s) URL they give, including addresses on your own network.

### GitHub

Workspace owners and admins: `POST /workspaces/{slug}/github` connects (or makes a new link and secret) and answers `{path, secret}` once; `GET` shows `{connected, path, close_on_merge, last_event_at}`; `PATCH {close_on_merge}`; `DELETE` disconnects. In GitHub, add a repository webhook to `<public_url><path>` with that secret, content type `application/json`, events "Pull requests" and "Pushes".

`POST /api/v1/integrations/github/{token}` checks `X-Hub-Signature-256`, then: a pull request whose title, description or branch mentions a key (`WEB-12`, any case) gets a link on that work item ("PR #42: Fix login (open \| draft \| merged \| closed)", renamed as the pull request changes); each pushed commit mentioning a key gets a "Commit abc1234: …" link. With `close_on_merge` (default on), a merged pull request that says "fixes", "closes" or "resolves" right before a key moves that item to the project's completed state. Changes appear as the "GitHub" bot.

### Inbox, drafts, preferences

| Method | Path | What |
|---|---|---|
| GET | `/workspaces/{slug}/notifications?filter=unread\|all\|snoozed\|archived` | your notifications: `kind` (`assigned`, `mentioned`, `commented`, `updated`, `archived`), `field`, values, the item's key and title |
| GET | `/workspaces/{slug}/notifications/count` | `{unread}` |
| POST | `/workspaces/{slug}/notifications/{nid}/{action}` | `read`, `unread`, `archive`, `unarchive`, `snooze` (`{until: "YYYY-MM-DD" or timestamp}`), `unsnooze` |
| POST | `/workspaces/{slug}/notifications/read-all` | marks what the inbox shows (not snoozed ones) |
| GET | `/workspaces/{slug}/my-work?kind=assigned\|created\|subscribed&include_closed=` | across the projects you joined |
| GET | `/workspaces/{slug}/work-items` | workspace views: published, unarchived items of the live projects you are a member of and may read, newest first, each with `identifier` and `project_name`. Takes the project list's filters (`state_group`, `priority`, `assignee`, `unassigned`, `label`, `created_by`, `due_before`/`after`, `start_before`/`after`, `q`) plus `project` (ids, comma-separated), `subscribed=true` (you follow them) and `subscriber` (a person's id: they follow them); `archived=true` is refused (`400`). Pages of `limit` (default 1000) with `X-Next-Cursor` |
| GET / POST | `/workspaces/{slug}/views` | workspace views: shared ones and your own (workspace guests: only their own). `POST {name, description?, is_shared? (default true), config}`, any workspace member |
| PATCH / DELETE | `/workspaces/{slug}/views/{vid}` | `PATCH {name?, description?, is_shared?, config?}` by the owner only; `DELETE` by the owner or a workspace admin |
| GET | `/workspaces/{slug}/home/widgets` | your home widgets `[{key, is_enabled, sort_order}]`, highest `sort_order` first; `key` is `quick_links`, `recents` or `stickies` |
| PATCH | `/workspaces/{slug}/home/widgets/{key}` | `{is_enabled?, sort_order?}` → the list |
| GET / POST | `/workspaces/{slug}/quick-links` | your quick links, newest first; `POST {url, title?}` (`example.com` becomes `http://example.com`; only http(s); the same address twice is `409`) |
| PATCH / DELETE | `/workspaces/{slug}/quick-links/{id}` | your own only |
| GET | `/workspaces/{slug}/recents?entity=work_item\|page\|project` | what you opened lately, most recent first (the newest 20 are kept), with what each shows: a work item's `identifier`, `sequence`, `title`, `priority`, `assignee_ids`, `state{name, color, group}`; a page's `title` and `project_name`; a project's `name`, `identifier`, `icon`, `cover_color`. Things you can no longer see are left out |
| POST | `/workspaces/{slug}/recents` | `{entity, id, project_id?}` records a visit (the web app sends it when you open a project, work item or page); only things you may see (`404` otherwise) |
| GET / POST | `/workspaces/{slug}/stickies?q=&limit=&offset=` | your stickies, highest `sort_order` first (new ones go on top), `q` matches their text, `limit` 30 by default (at most 100). `POST {description_html?, description_json?, color?}`; colours: `gray`, `peach`, `pink`, `orange`, `green`, `light-blue`, `dark-blue`, `purple` (a random one when left out) |
| PATCH / DELETE | `/workspaces/{slug}/stickies/{id}` | `{description_html?, description_json?, color?, sort_order?}`; your own only. Home changes are personal: they make no realtime events or webhooks |
| POST | `/workspaces/{slug}/ai` | members and admins (not guests), 60 an hour each, once an instance admin saved a key: `{task, prompt?}` → `{response, response_html}`; the service gets the task, a newline, then the prompt |
| GET | `/workspaces/{slug}/unsplash?query=&page=` | Unsplash's latest photos or a search, 20 a page: `[{id, thumb, regular, description, author, author_url, download_location}]`; empty while Unsplash is off |
| GET | `/workspaces/{slug}/drafts` | your drafts. Create one with `is_draft: true`; drafts are private, not listed, searched or counted, and notify nobody |
| POST | `/workspaces/{slug}/projects/{id}/work-items/{wid}/publish` | publish your draft |
| GET / PATCH | `/me/preferences` | `{email_notifications, email_enabled}` |

Notifications go to an item's subscribers (creator, assignees, commenters, mentioned people, anyone who subscribed) except the person who made the change, and only while they can read the project. With SMTP set up, unread notifications older than ten minutes are emailed once as a digest.

### Single sign-on

With `[oidc]` in config.toml, `GET /instance` includes `sso_name`, and `GET /auth/oidc/start?next=/path` starts the authorization code flow with PKCE; the provider returns to `/auth/oidc/callback`. People are matched by verified email. New accounts are created only when the signup policy is `open`, or `invite` and the email has a pending invite. Errors come back as `/login?sso_error=…`.

The providers an instance admin turns on (`/admin/auth`) are listed in `GET /instance` as `providers[{key, name}]`, next to `password_enabled`, `ai_enabled` and `unsplash_enabled`. `GET /auth/oauth/{google|github|gitlab|gitea}/start?next=/path` starts their sign-in and they return to `/auth/oauth/{provider}/callback`. Google, GitLab and Gitea use OpenID Connect (PKCE, a verified email); GitHub uses OAuth with the scopes `read:user user:email` (plus `read:org` with an organization), takes the account's primary email only if it is verified, and with an organization set checks membership. Matching and new accounts follow the same rules as above. With password sign-in off, `POST /auth/login` answers `403` except for instance admins (the way back in if a provider breaks), and password sign-up is refused.

## MCP (agents)

`POST /mcp` is a Model Context Protocol server (streamable HTTP, JSON responses, protocol `2025-06-18`, also `2025-03-26` and `2024-11-05`). Authenticate with a bearer token, usually a bot's. Every tool calls this REST API as that account, so an agent can do exactly what its account may do, and its changes show under its name.

| Tool | Does |
|---|---|
| `list_workspaces`, `list_projects` | where the account can work |
| `list_states`, `list_members`, `list_labels`, `list_cycles` | names to use in other tools |
| `list_work_items` | filter by state, state group, priority, assignee (`me`), text |
| `get_work_item` | one item by key, with description and comments as text |
| `search` | full text over work items and pages |
| `create_work_item`, `update_work_item` | title, description (plain text), state and labels by name, priority, assignees (`me`, emails, names), dates, estimate, parent key |
| `add_comment` | plain text |
| `list_pages`, `get_page` | a project's pages; one page's text |
| `create_page` | title and plain-text content (blank lines separate paragraphs), optional parent |

Example client configuration (Claude Code):

```sh
claude mcp add --transport http nextplane https://plane.example.com/mcp --header "Authorization: Bearer np_…"
```

## Realtime

`GET /api/v1/ws` upgrades to a WebSocket (session cookie or bearer token). Send `{"subscribe": "<workspace slug>"}`; the server answers `{"subscribed": "<slug>"}` (or `null` if you are not a member). After every successful change in that workspace you receive:

```json
{"workspace": "acme", "project_id": "…", "work_item_id": "…", "area": "comment", "method": "POST", "actor": "…", "client": "…"}
```

`area` is one of `workspace`, `member`, `invite`, `project`, `state`, `label`, `work_item`, `relation`, `comment`, `reaction`, `attachment`, `link`, `cycle`, `module`, `view`, `page`. Events of private projects reach only people who may see them. `{"area": "resync"}` means you fell behind: refetch everything. Events carry no content — refetch what you show.

## Example: an agent picks up work

```sh
TOKEN=np_…   # from Workspace settings › Agents and bots
H="Authorization: Bearer $TOKEN"
B=https://plane.example.com/api/v1/workspaces/acme

# Join the project, list unstarted urgent items, move one to "In Progress", report back.
curl -s -X POST -H "$H" $B/projects/$PID/join
curl -s -H "$H" "$B/projects/$PID/work-items?state_group=unstarted&priority=urgent"
curl -s -X PATCH -H "$H" -H 'Content-Type: application/json' \
  -d "{\"state_id\": \"$IN_PROGRESS\"}" $B/projects/$PID/work-items/$WID
curl -s -X POST -H "$H" -H 'Content-Type: application/json' \
  -d '{"body_html": "<p>Fixed in commit abc123, tests pass.</p>"}' $B/projects/$PID/work-items/$WID/comments
```
