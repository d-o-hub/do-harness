-- 0003_persist.sql
-- Unique error-signature keys so the fail-fast policy can count consecutive
-- attempts with an upsert instead of a scan.

DROP INDEX IF EXISTS idx_error_signatures_signature;
CREATE UNIQUE INDEX idx_error_signatures_signature ON error_signatures(signature);