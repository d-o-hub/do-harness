-- 0021_error_signature_scope.sql
-- Fail-fast strikes become workstream-scoped, exactly like beats.
--
-- Strikes were keyed by `task_id` alone, which is NULL for every branch-scoped
-- and global run: three failures on one branch halted the fail-fast gate of
-- every other branch, and `metrics` attributed another workstream's strikes to
-- the current snapshot. The row key becomes the scope string beats store
-- (`task:<id>`, `branch:<name>`, `global`), so one workstream can never strike
-- out another.

ALTER TABLE error_signatures ADD COLUMN scope TEXT;

-- Every legacy row maps 1:1 onto a scope (`task:<id>` rows are unique per the
-- old partial index, and NULL task ids were already folded to one row per
-- signature), so this backfill cannot create a duplicate key.
UPDATE error_signatures
   SET scope = CASE
       WHEN task_id IS NULL THEN 'global'
       ELSE 'task:' || task_id
   END
 WHERE scope IS NULL;

-- The old partial indexes cannot express per-branch rows: the global one is
-- unique on `signature` for NULL task ids and would reject a second branch's
-- row for the same sensor.
DROP INDEX IF EXISTS idx_error_signatures_signature_task;
DROP INDEX IF EXISTS idx_error_signatures_signature_global;

CREATE UNIQUE INDEX IF NOT EXISTS idx_error_signatures_signature_scope
    ON error_signatures(signature, scope);
