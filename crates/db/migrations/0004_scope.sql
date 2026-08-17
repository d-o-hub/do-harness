-- 0004_scope.sql
-- Scope error signatures by (signature, task_id) so the fail-fast strike
-- counter is per-task (NULL task_id = workspace-global) instead of global.

DROP INDEX IF EXISTS idx_error_signatures_signature;
CREATE UNIQUE INDEX idx_error_signatures_signature_task
    ON error_signatures(signature, task_id);
