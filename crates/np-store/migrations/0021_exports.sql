-- 0021: workspace exports (SPRINT S3.7). Each export is made on request and downloaded at once;
-- this keeps who exported what, so it can be downloaded again (with the data of that moment).
CREATE TABLE exports (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    workspace_id BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id      BLOB NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    format       TEXT NOT NULL CHECK (format IN ('csv', 'json', 'xlsx')),
    -- Project ids as a JSON array; empty means every project the person could export.
    project_ids  TEXT NOT NULL DEFAULT '[]',
    items        INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);
CREATE INDEX exports_user ON exports (workspace_id, user_id, created_at);
