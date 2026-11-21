-- 0020: a project's cover image (SPRINT S3.7): an uploaded file or an Unsplash photo, with the
-- photographer's credit for Unsplash photos (JSON {name, url}).
ALTER TABLE projects ADD COLUMN cover_image TEXT;
ALTER TABLE projects ADD COLUMN cover_credit TEXT;
