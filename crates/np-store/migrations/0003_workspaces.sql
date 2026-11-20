-- 0003: workspaces, members, invites, password resets, signup policy (SPRINT S1.1).

ALTER TABLE instance ADD COLUMN signup_policy TEXT NOT NULL DEFAULT 'invite'
    CHECK (signup_policy IN ('closed', 'invite', 'open'));

CREATE TABLE workspaces (
    id                          BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    slug                        TEXT NOT NULL UNIQUE,
    name                        TEXT NOT NULL,
    logo                        TEXT,
    members_can_create_projects INTEGER NOT NULL DEFAULT 0 CHECK (members_can_create_projects IN (0, 1)),
    created_by                  BLOB NOT NULL REFERENCES users (id),
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL,
    deleted_at                  TEXT
);

-- A membership that ends is deactivated, not erased, so history keeps its people.
CREATE TABLE workspace_members (
    workspace_id   BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id        BLOB NOT NULL REFERENCES users (id),
    role           TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member', 'guest')),
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    deactivated_at TEXT,
    PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX workspace_members_user ON workspace_members (user_id);

CREATE TABLE invites (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    workspace_id BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    email        TEXT NOT NULL COLLATE NOCASE,
    role         TEXT NOT NULL CHECK (role IN ('admin', 'member', 'guest')),
    token_hash   BLOB NOT NULL UNIQUE,
    invited_by   BLOB NOT NULL REFERENCES users (id),
    expires_at   TEXT NOT NULL,
    accepted_at  TEXT,
    revoked_at   TEXT,
    created_at   TEXT NOT NULL
);
CREATE INDEX invites_workspace ON invites (workspace_id);

CREATE TABLE password_resets (
    id         BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    user_id    BLOB NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash BLOB NOT NULL UNIQUE,
    created_by BLOB REFERENCES users (id),
    expires_at TEXT NOT NULL,
    used_at    TEXT,
    created_at TEXT NOT NULL
);
