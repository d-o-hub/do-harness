-- 0022_dora_rework.sql
-- Deployment rework rate: deploys whose own lead range contains a revert.
--
-- The fifth DORA metric derives from the same revert predicate as failure
-- attribution, scanned over the deploy's own range instead of its failure
-- range. Backfilled as 0: rework is a property of a fresh derivation, and a
-- snapshot row that predates the rule must not claim a measurement it never
-- took. The writer (`crates/db/src/repo_dora.rs`) always sets the column, so
-- 0 appears only on pre-rule rows.

ALTER TABLE dora_snapshots ADD COLUMN deploys_rework INTEGER NOT NULL DEFAULT 0;
