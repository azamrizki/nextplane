-- 0006: comments, reactions, files, links, subscriptions (SPRINT S1.4).

CREATE TABLE comments (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    work_item_id BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    author_id    BLOB NOT NULL REFERENCES users (id),
    body_json    TEXT,
    body_html    TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    edited_at    TEXT,
    hidden_at    TEXT,
    hidden_by    BLOB REFERENCES users (id),
    deleted_at   TEXT
);
CREATE INDEX comments_item ON comments (work_item_id, created_at);

CREATE TABLE reactions (
    id          BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    target_kind TEXT NOT NULL CHECK (target_kind IN ('work_item', 'comment')),
    target_id   BLOB NOT NULL,
    user_id     BLOB NOT NULL REFERENCES users (id),
    emoji       TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    UNIQUE (target_kind, target_id, user_id, emoji)
);
CREATE INDEX reactions_target ON reactions (target_kind, target_id);

-- Uploaded files. `purpose` says what a file belongs to; bytes live in <data>/uploads.
CREATE TABLE files (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    workspace_id BLOB REFERENCES workspaces (id) ON DELETE CASCADE,
    project_id   BLOB REFERENCES projects (id) ON DELETE CASCADE,
    work_item_id BLOB REFERENCES work_items (id) ON DELETE CASCADE,
    purpose      TEXT NOT NULL CHECK (purpose IN ('attachment', 'editor', 'avatar', 'logo')),
    uploaded_by  BLOB NOT NULL REFERENCES users (id),
    file_name    TEXT NOT NULL,
    mime         TEXT NOT NULL,
    size         INTEGER NOT NULL,
    path         TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    deleted_at   TEXT
);
CREATE INDEX files_item ON files (work_item_id);

CREATE TABLE work_item_links (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    work_item_id BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    url          TEXT NOT NULL,
    title        TEXT NOT NULL DEFAULT '',
    created_by   BLOB NOT NULL REFERENCES users (id),
    created_at   TEXT NOT NULL
);

CREATE TABLE subscriptions (
    work_item_id BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    user_id      BLOB NOT NULL REFERENCES users (id),
    created_at   TEXT NOT NULL,
    PRIMARY KEY (work_item_id, user_id)
);
CREATE INDEX subscriptions_user ON subscriptions (user_id);
