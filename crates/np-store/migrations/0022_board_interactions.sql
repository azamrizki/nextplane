-- 0022: comments, reactions and votes on public boards (SPRINT S3.7), each switched on per
-- project. Visitors read without an account; to comment, react or vote they sign in. Only
-- comments marked public show on the board; comments made in the app stay private unless
-- someone makes them public.
ALTER TABLE projects ADD COLUMN board_comments INTEGER NOT NULL DEFAULT 0;
ALTER TABLE projects ADD COLUMN board_reactions INTEGER NOT NULL DEFAULT 0;
ALTER TABLE projects ADD COLUMN board_votes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE comments ADD COLUMN is_public INTEGER NOT NULL DEFAULT 0;

CREATE TABLE work_item_votes (
    work_item_id BLOB NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    user_id      BLOB NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    value        INTEGER NOT NULL CHECK (value IN (-1, 1)),
    created_at   TEXT NOT NULL,
    PRIMARY KEY (work_item_id, user_id)
);
