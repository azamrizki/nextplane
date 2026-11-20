-- 0005: work items, assignees, labels, relations, activity (SPRINT S1.3).

CREATE TABLE work_items (
    id               BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    project_id       BLOB NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    sequence         INTEGER NOT NULL,
    title            TEXT NOT NULL,
    description_json TEXT,
    description_html TEXT NOT NULL DEFAULT '',
    state_id         BLOB NOT NULL REFERENCES states (id),
    priority         TEXT NOT NULL DEFAULT 'none' CHECK (priority IN ('urgent', 'high', 'medium', 'low', 'none')),
    start_date       TEXT,
    due_date         TEXT,
    estimate         TEXT,
    parent_id        BLOB REFERENCES work_items (id) ON DELETE SET NULL,
    sort_order       REAL NOT NULL,
    created_by       BLOB NOT NULL REFERENCES users (id),
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    completed_at     TEXT,
    archived_at      TEXT,
    deleted_at       TEXT,
    UNIQUE (project_id, sequence)
);
CREATE INDEX work_items_project ON work_items (project_id, deleted_at, archived_at);
CREATE INDEX work_items_state ON work_items (state_id);
CREATE INDEX work_items_parent ON work_items (parent_id);

CREATE TABLE work_item_assignees (
    work_item_id BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    user_id      BLOB NOT NULL REFERENCES users (id),
    PRIMARY KEY (work_item_id, user_id)
);
CREATE INDEX work_item_assignees_user ON work_item_assignees (user_id);

CREATE TABLE work_item_labels (
    work_item_id BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    label_id     BLOB NOT NULL REFERENCES labels (id) ON DELETE CASCADE,
    PRIMARY KEY (work_item_id, label_id)
);

-- `blocked_by` is the reverse of `blocks` and is not stored.
CREATE TABLE work_item_relations (
    id         BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    from_id    BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    to_id      BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('blocks', 'relates_to', 'duplicate_of')),
    created_by BLOB NOT NULL REFERENCES users (id),
    created_at TEXT NOT NULL,
    UNIQUE (from_id, to_id, kind)
);

CREATE TABLE work_item_activity (
    id           BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    work_item_id BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    actor_id     BLOB NOT NULL REFERENCES users (id),
    verb         TEXT NOT NULL,
    field        TEXT,
    old_value    TEXT,
    new_value    TEXT,
    created_at   TEXT NOT NULL
);
CREATE INDEX work_item_activity_item ON work_item_activity (work_item_id, created_at);
