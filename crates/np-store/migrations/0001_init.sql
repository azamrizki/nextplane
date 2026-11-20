-- 0001: the instance row, users and sessions (SPEC §7, SPRINT S0.1).
-- IDs are UUIDv7 as 16-byte BLOBs; timestamps are UTC RFC 3339 text.

CREATE TABLE instance (
    id                 INTEGER PRIMARY KEY CHECK (id = 1),
    name               TEXT    NOT NULL DEFAULT '',
    setup_completed_at TEXT,
    created_at         TEXT    NOT NULL,
    updated_at         TEXT    NOT NULL
);

CREATE TABLE users (
    id                BLOB    PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    email             TEXT    NOT NULL COLLATE NOCASE UNIQUE,
    name              TEXT    NOT NULL DEFAULT '',
    avatar            TEXT,
    password_hash     TEXT,
    is_instance_admin INTEGER NOT NULL DEFAULT 0 CHECK (is_instance_admin IN (0, 1)),
    is_bot            INTEGER NOT NULL DEFAULT 0 CHECK (is_bot IN (0, 1)),
    created_at        TEXT    NOT NULL,
    updated_at        TEXT    NOT NULL,
    deleted_at        TEXT
);

CREATE TABLE sessions (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    user_id      BLOB NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash   BLOB NOT NULL UNIQUE,
    expires_at   TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    user_agent   TEXT NOT NULL DEFAULT '',
    ip           TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL
);

CREATE INDEX sessions_user_id ON sessions (user_id);
CREATE INDEX sessions_expires_at ON sessions (expires_at);
