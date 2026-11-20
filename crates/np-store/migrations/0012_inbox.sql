-- 0012: notifications, email preference, draft work items (SPRINT S2.4).
CREATE TABLE notifications (
    id            BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    user_id       BLOB NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    workspace_id  BLOB NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    project_id    BLOB NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    work_item_id  BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    actor_id      BLOB NOT NULL REFERENCES users (id),
    kind          TEXT NOT NULL CHECK (kind IN ('assigned', 'mentioned', 'commented', 'updated', 'archived')),
    field         TEXT,
    old_value     TEXT,
    new_value     TEXT,
    comment_id    BLOB,
    created_at    TEXT NOT NULL,
    read_at       TEXT,
    archived_at   TEXT,
    snoozed_until TEXT,
    emailed_at    TEXT
);
CREATE INDEX notifications_inbox ON notifications (user_id, workspace_id, archived_at, created_at);

ALTER TABLE users ADD COLUMN email_notifications INTEGER NOT NULL DEFAULT 1;
ALTER TABLE work_items ADD COLUMN is_draft INTEGER NOT NULL DEFAULT 0;
