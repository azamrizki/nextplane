-- 0019: instance settings kept as key/value pairs (SPRINT S3.7): sign-in providers, password
-- sign-in, the AI assistant and Unsplash. Secrets (client secrets, API keys) are never sent
-- back to a browser; the API only says whether one is set.
CREATE TABLE instance_config (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
