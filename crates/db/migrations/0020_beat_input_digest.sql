-- 0020_beat_input_digest.sql
-- Sensor input identity on beats, for `verify --unchanged` reuse.
--
-- A sensor that declares `inputs = [...]` gets one SHA-256 identity of every
-- declared input (paths, content, mode, config, baselines, HEAD) recorded on
-- its clean passing beats. A later run reuses the beat only when the current
-- identity equals the stored one; NULL (the pre-migration default, or a
-- sensor that declines caching) is never reused.

ALTER TABLE beats ADD COLUMN input_digest TEXT;
