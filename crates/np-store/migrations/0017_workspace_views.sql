-- 0017: workspace views (SPRINT S3.7) are saved views with no project; they get a description.
ALTER TABLE views ADD COLUMN description TEXT NOT NULL DEFAULT '';
CREATE INDEX views_workspace ON views (workspace_id, deleted_at) WHERE project_id IS NULL;
