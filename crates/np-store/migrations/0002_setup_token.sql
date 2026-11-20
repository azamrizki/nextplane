-- 0002: the one-time setup link (SPRINT S0.2). Only its SHA-256 hash is stored.
ALTER TABLE instance ADD COLUMN setup_token_hash BLOB;
ALTER TABLE instance ADD COLUMN setup_token_expires_at TEXT;
