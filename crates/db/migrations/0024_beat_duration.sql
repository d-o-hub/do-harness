-- 0024_beat_duration.sql
-- Persist the sensor runner's measured wall-clock duration on each recorded
-- sensor beat. Before this migration the recorder stamped `started_at` and
-- `completed_at` with the same post-run time, so the report's derived
-- `duration_ms` was always 0 ms for real executions. The measured value is
-- additive and optional: legacy rows keep NULL and the export falls back to
-- deriving from the timestamps, rendering unknown when only a single
-- recording stamp exists.
ALTER TABLE beats ADD COLUMN duration_ms INTEGER;
