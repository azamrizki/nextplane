-- 0011: saved views (SPRINT S2.3). A view stores filters, layout and display settings as the
-- web app's JSON; personal views are seen only by their owner.
CREATE TABLE views (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    workspace_id BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    project_id   BLOB REFERENCES projects (id) ON DELETE CASCADE,
    owner_id     BLOB NOT NULL REFERENCES users (id),
    name         TEXT NOT NULL,
    is_shared    INTEGER NOT NULL DEFAULT 0 CHECK (is_shared IN (0, 1)),
    config       TEXT NOT NULL DEFAULT '{}',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    deleted_at   TEXT
);
CREATE INDEX views_project ON views (project_id, deleted_at);
