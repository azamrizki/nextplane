-- 0015: public boards, webhooks and the GitHub link (SPRINT S3.5).

-- A read-only public link to a project's board; NULL while it is not published.
ALTER TABLE projects ADD COLUMN public_board_token TEXT;
CREATE UNIQUE INDEX projects_public_board ON projects (public_board_token) WHERE public_board_token IS NOT NULL;

CREATE TABLE webhooks (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    workspace_id BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    url          TEXT NOT NULL,
    -- Signs every delivery (HMAC-SHA256); kept to sign, shown to admins only.
    secret       TEXT NOT NULL,
    -- Comma-separated areas (work_item, comment, cycle, …) or '*'.
    events       TEXT NOT NULL DEFAULT '*',
    active       INTEGER NOT NULL DEFAULT 1,
    created_by   BLOB REFERENCES users (id),
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX webhooks_workspace ON webhooks (workspace_id);

CREATE TABLE webhook_deliveries (
    id              BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    webhook_id      BLOB NOT NULL REFERENCES webhooks (id) ON DELETE CASCADE,
    event           TEXT NOT NULL,
    payload         TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'delivered', 'failed')),
    attempts        INTEGER NOT NULL DEFAULT 0,
    response_code   INTEGER,
    error           TEXT,
    created_at      TEXT NOT NULL,
    last_attempt_at TEXT,
    next_attempt_at TEXT
);
CREATE INDEX webhook_deliveries_due ON webhook_deliveries (status, next_attempt_at);
CREATE INDEX webhook_deliveries_hook ON webhook_deliveries (webhook_id, created_at);

-- One GitHub connection per workspace: GitHub posts to /api/v1/integrations/github/{token}
-- and signs with the secret.
CREATE TABLE github_connections (
    workspace_id   BLOB PRIMARY KEY NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    token          TEXT NOT NULL UNIQUE,
    secret         TEXT NOT NULL,
    bot_id         BLOB NOT NULL REFERENCES users (id),
    close_on_merge INTEGER NOT NULL DEFAULT 1,
    created_by     BLOB REFERENCES users (id),
    created_at     TEXT NOT NULL,
    last_event_at  TEXT
);
