-- 0008: an index in list order, so a page of work items is read without sorting the whole
-- project (SPRINT S1.6 performance check).
CREATE INDEX work_items_order ON work_items (project_id, sort_order, sequence);
