-- 0019_beat_scope.sql
-- Scope beats to the workstream that produced them.
--
-- A NULL `task_id` cannot distinguish a branch-scoped beat from a genuinely
-- unscoped one, and `metrics` aggregated both — beats from unrelated
-- workstreams summed into the same sensor statistics. `--record` now scopes to
-- the current git branch by default, to a task when `--task` is given, and to
-- `global` only on request. Existing rows are backfilled so history keeps its
-- meaning.

ALTER TABLE beats ADD COLUMN scope TEXT NOT NULL DEFAULT 'global';

UPDATE beats SET scope = 'task:' || task_id WHERE task_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_beats_scope_sensor ON beats(scope, sensor_name);
