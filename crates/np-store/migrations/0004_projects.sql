-- 0004: projects, project members, states, labels (SPRINT S1.2).

CREATE TABLE projects (
    id                  BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    workspace_id        BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    identifier          TEXT NOT NULL,
    name                TEXT NOT NULL,
    description         TEXT NOT NULL DEFAULT '',
    icon                TEXT,
    cover_color         TEXT,
    lead_id             BLOB REFERENCES users (id),
    default_assignee_id BLOB REFERENCES users (id),
    is_private          INTEGER NOT NULL DEFAULT 0 CHECK (is_private IN (0, 1)),
    next_sequence       INTEGER NOT NULL DEFAULT 1,
    estimate_type       TEXT NOT NULL DEFAULT 'none' CHECK (estimate_type IN ('none', 'points', 'tshirt')),
    estimate_values     TEXT NOT NULL DEFAULT '[]',
    feature_cycles      INTEGER NOT NULL DEFAULT 1,
    feature_modules     INTEGER NOT NULL DEFAULT 1,
    feature_pages       INTEGER NOT NULL DEFAULT 1,
    feature_intake      INTEGER NOT NULL DEFAULT 0,
    created_by          BLOB NOT NULL REFERENCES users (id),
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    archived_at         TEXT,
    deleted_at          TEXT
);
-- Identifiers and names are unique among projects that are not deleted.
CREATE UNIQUE INDEX projects_identifier ON projects (workspace_id, identifier) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX projects_name ON projects (workspace_id, name COLLATE NOCASE) WHERE deleted_at IS NULL;

CREATE TABLE project_members (
    project_id BLOB NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    user_id    BLOB NOT NULL REFERENCES users (id),
    role       TEXT NOT NULL CHECK (role IN ('admin', 'contributor', 'commenter', 'guest')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (project_id, user_id)
);
CREATE INDEX project_members_user ON project_members (user_id);

CREATE TABLE states (
    id         BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    project_id BLOB NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL,
    grp        TEXT NOT NULL CHECK (grp IN ('backlog', 'unstarted', 'started', 'completed', 'cancelled')),
    sort_order REAL NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX states_name ON states (project_id, name COLLATE NOCASE);

CREATE TABLE labels (
    id         BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    project_id BLOB NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    parent_id  BLOB REFERENCES labels (id) ON DELETE SET NULL,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL,
    sort_order REAL NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX labels_name ON labels (project_id, name COLLATE NOCASE);
