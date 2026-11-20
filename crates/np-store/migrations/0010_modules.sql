-- 0010: modules (SPRINT S2.2). A work item can be in many modules.
CREATE TABLE modules (
    id          BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    project_id  BLOB NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'backlog'
                CHECK (status IN ('backlog', 'planned', 'in_progress', 'paused', 'completed', 'cancelled')),
    lead_id     BLOB REFERENCES users (id),
    start_date  TEXT,
    target_date TEXT,
    created_by  BLOB NOT NULL REFERENCES users (id),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    archived_at TEXT,
    deleted_at  TEXT,
    CHECK (start_date IS NULL OR target_date IS NULL OR start_date <= target_date)
);
CREATE UNIQUE INDEX modules_name ON modules (project_id, name COLLATE NOCASE) WHERE deleted_at IS NULL;

CREATE TABLE module_members (
    module_id BLOB NOT NULL REFERENCES modules (id) ON DELETE CASCADE,
    user_id   BLOB NOT NULL REFERENCES users (id),
    PRIMARY KEY (module_id, user_id)
);

CREATE TABLE work_item_modules (
    work_item_id BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    module_id    BLOB NOT NULL REFERENCES modules (id) ON DELETE CASCADE,
    PRIMARY KEY (work_item_id, module_id)
);
CREATE INDEX work_item_modules_module ON work_item_modules (module_id);

CREATE TABLE module_links (
    id         BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    module_id  BLOB NOT NULL REFERENCES modules (id) ON DELETE CASCADE,
    url        TEXT NOT NULL,
    title      TEXT NOT NULL DEFAULT '',
    created_by BLOB NOT NULL REFERENCES users (id),
    created_at TEXT NOT NULL
);
