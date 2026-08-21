-- 0007_fk_strike_index.sql
-- Make strike uniqueness enforceable for workspace-global signatures and
-- clean up duplicates that accumulated while the old index was blind to
-- NULL task_id (SQLite UNIQUE treats NULLs as distinct).

-- Fold duplicate global strikes into the oldest row: sum the newer rows'
-- attempt counts into it before deleting them.
UPDATE error_signatures
SET attempt_count = attempt_count + COALESCE((
    SELECT SUM(s2.attempt_count) FROM error_signatures AS s2
    WHERE s2.task_id IS NULL
      AND s2.signature = error_signatures.signature
      AND s2.id > error_signatures.id
), 0)
WHERE task_id IS NULL
  AND EXISTS (
    SELECT 1 FROM error_signatures AS s3
    WHERE s3.task_id IS NULL
      AND s3.signature = error_signatures.signature
      AND s3.id < error_signatures.id
);

DELETE FROM error_signatures
WHERE task_id IS NULL
  AND id NOT IN (
    SELECT MIN(id) FROM error_signatures WHERE task_id IS NULL GROUP BY signature
);

-- Per-task strikes keep their (signature, task_id) uniqueness; global strikes
-- get a dedicated partial index so NULL task_id rows are covered too.
DROP INDEX IF EXISTS idx_error_signatures_signature_task;
CREATE UNIQUE INDEX idx_error_signatures_signature_task
    ON error_signatures(signature, task_id) WHERE task_id IS NOT NULL;
CREATE UNIQUE INDEX idx_error_signatures_signature_global
    ON error_signatures(signature) WHERE task_id IS NULL;
