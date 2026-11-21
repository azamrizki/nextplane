-- 0016: project automations (SPRINT S3.7). Months are 30 days; 0 is off.
-- archive_in: archive work items closed (completed or cancelled) for that long.
-- close_in: move work items untouched for that long to close_state_id, a cancelled state.
ALTER TABLE projects ADD COLUMN archive_in INTEGER NOT NULL DEFAULT 0 CHECK (archive_in BETWEEN 0 AND 12);
ALTER TABLE projects ADD COLUMN close_in INTEGER NOT NULL DEFAULT 0 CHECK (close_in BETWEEN 0 AND 12);
ALTER TABLE projects ADD COLUMN close_state_id BLOB REFERENCES states (id) ON DELETE SET NULL;
