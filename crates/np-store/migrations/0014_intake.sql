-- 0014: intake (SPRINT S3.3). An intake submission is a work item with is_draft = 2:
-- work_items.is_draft is 0 for normal items, 1 for the creator's draft, 2 while in intake
-- (pending, snoozed, declined or duplicate). Every list, count and search already keeps
-- only is_draft = 0, so submissions stay out of the project until accepted.
CREATE TABLE intake_items (
    work_item_id    BLOB PRIMARY KEY NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    project_id      BLOB NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'declined', 'duplicate', 'snoozed')),
    snoozed_until   TEXT,
    duplicate_of    BLOB REFERENCES work_items (id) ON DELETE SET NULL,
    reason          TEXT,
    source          TEXT NOT NULL DEFAULT 'app' CHECK (source IN ('app', 'form')),
    submitter_name  TEXT,
    submitter_email TEXT,
    triaged_by      BLOB REFERENCES users (id),
    triaged_at      TEXT,
    created_at      TEXT NOT NULL
);
CREATE INDEX intake_items_project ON intake_items (project_id, status, created_at);

-- The public intake form: a link with this token, NULL while the form is off.
ALTER TABLE projects ADD COLUMN intake_form_token TEXT;
CREATE UNIQUE INDEX projects_intake_form ON projects (intake_form_token) WHERE intake_form_token IS NOT NULL;
