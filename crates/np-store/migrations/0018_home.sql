-- 0018: the home page's widgets (SPRINT S3.7): per person and workspace, as in Plane.

-- Which widgets someone shows on their home page, and in what order (highest first).
CREATE TABLE home_widgets (
    user_id      BLOB NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    workspace_id BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    key          TEXT NOT NULL CHECK (key IN ('quick_links', 'recents', 'stickies')),
    is_enabled   INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    sort_order   REAL NOT NULL,
    PRIMARY KEY (user_id, workspace_id, key)
);

-- Personal bookmarks.
CREATE TABLE quick_links (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    workspace_id BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id      BLOB NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    url          TEXT NOT NULL,
    title        TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    UNIQUE (workspace_id, user_id, url)
);

-- What someone opened lately; the newest 20 per person and workspace are kept.
CREATE TABLE recent_visits (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    workspace_id BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id      BLOB NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    project_id   BLOB REFERENCES projects (id) ON DELETE CASCADE,
    entity       TEXT NOT NULL CHECK (entity IN ('work_item', 'page', 'project')),
    entity_id    BLOB NOT NULL,
    visited_at   TEXT NOT NULL,
    UNIQUE (workspace_id, user_id, entity, entity_id)
);
CREATE INDEX recent_visits_user ON recent_visits (workspace_id, user_id, visited_at);

-- Personal sticky notes: rich text (bold, italic, checklists) on a coloured card.
CREATE TABLE stickies (
    id               BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    workspace_id     BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id          BLOB NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    description_json TEXT,
    description_html TEXT NOT NULL DEFAULT '',
    -- Plain text of the note, for search.
    text             TEXT NOT NULL DEFAULT '',
    color            TEXT NOT NULL DEFAULT 'gray',
    sort_order       REAL NOT NULL,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);
CREATE INDEX stickies_user ON stickies (workspace_id, user_id, sort_order);
