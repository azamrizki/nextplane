-- 0007: API tokens and full-text search (SPRINT S1.5).

CREATE TABLE api_tokens (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    user_id      BLOB NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    token_hash   BLOB NOT NULL UNIQUE,
    scope        TEXT NOT NULL CHECK (scope IN ('read', 'write')),
    created_by   BLOB NOT NULL REFERENCES users (id),
    expires_at   TEXT,
    last_used_at TEXT,
    revoked_at   TEXT,
    created_at   TEXT NOT NULL
);
CREATE INDEX api_tokens_user ON api_tokens (user_id);

-- Bots belong to the workspace whose admin made them.
ALTER TABLE users ADD COLUMN bot_workspace_id BLOB REFERENCES workspaces (id);

-- Plain text of work items and comments, kept up to date by the application.
CREATE VIRTUAL TABLE search_fts USING fts5(
    kind UNINDEXED,
    ref_id UNINDEXED,
    project_id UNINDEXED,
    work_item_id UNINDEXED,
    title,
    body,
    tokenize = 'unicode61 remove_diacritics 2'
);
INSERT INTO search_fts (kind, ref_id, project_id, work_item_id, title, body)
    SELECT 'work_item', id, project_id, id, title, description_html FROM work_items WHERE deleted_at IS NULL;
INSERT INTO search_fts (kind, ref_id, project_id, work_item_id, title, body)
    SELECT 'comment', c.id, w.project_id, c.work_item_id, '', c.body_html
    FROM comments c JOIN work_items w ON w.id = c.work_item_id WHERE c.deleted_at IS NULL;
