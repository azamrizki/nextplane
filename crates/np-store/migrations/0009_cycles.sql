-- 0009: cycles (SPRINT S2.1). A work item is in at most one cycle.
CREATE TABLE cycles (
    id          BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    project_id  BLOB NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    start_date  TEXT,
    end_date    TEXT,
    owner_id    BLOB REFERENCES users (id),
    created_by  BLOB NOT NULL REFERENCES users (id),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    archived_at TEXT,
    deleted_at  TEXT,
    CHECK (start_date IS NULL OR end_date IS NULL OR start_date <= end_date)
);
CREATE INDEX cycles_project ON cycles (project_id, deleted_at);

ALTER TABLE work_items ADD COLUMN cycle_id BLOB REFERENCES cycles (id) ON DELETE SET NULL;
CREATE INDEX work_items_cycle ON work_items (cycle_id);
