-- 0013: pages and their versions (SPRINT S3.1). The Yjs document is the source of truth;
-- `html` is a sanitized snapshot for reading, search and agents.
CREATE TABLE pages (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    workspace_id BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    project_id   BLOB NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    parent_id    BLOB REFERENCES pages (id) ON DELETE SET NULL,
    owner_id     BLOB NOT NULL REFERENCES users (id),
    title        TEXT NOT NULL DEFAULT '',
    access       TEXT NOT NULL DEFAULT 'project' CHECK (access IN ('project', 'private')),
    ydoc         BLOB,
    html         TEXT NOT NULL DEFAULT '',
    sort_order   REAL NOT NULL DEFAULT 0,
    locked_at    TEXT,
    locked_by    BLOB REFERENCES users (id),
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    updated_by   BLOB REFERENCES users (id),
    archived_at  TEXT,
    deleted_at   TEXT
);
CREATE INDEX pages_project ON pages (project_id, deleted_at);

CREATE TABLE page_versions (
    id         BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    page_id    BLOB NOT NULL REFERENCES pages (id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    html       TEXT NOT NULL,
    created_by BLOB REFERENCES users (id),
    created_at TEXT NOT NULL
);
CREATE INDEX page_versions_page ON page_versions (page_id, created_at);
