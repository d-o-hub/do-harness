-- 0018_dora_snapshots.sql
-- DORA snapshot history: one row per derived measurement.
--
-- Deployment health is derived, not judged: every row carries the resolved
-- window, the source revision, the pinned policy fingerprint, and the full
-- derivation manifest (ranges scanned, filters, percentile method, incidents),
-- so a number can always be re-derived and a rewritten past is visible instead
-- of silently re-scored. All measured quantities are integers; the change
-- failure rate stays an exact deploys_failed/deploy_count pair.

CREATE TABLE IF NOT EXISTS dora_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    recorded_at INTEGER NOT NULL,
    source TEXT NOT NULL,
    window_days INTEGER NOT NULL,
    window_start INTEGER NOT NULL,
    window_end INTEGER NOT NULL,
    source_rev TEXT NOT NULL,
    deploy_count INTEGER NOT NULL,
    deploys_failed INTEGER NOT NULL,
    lead_samples INTEGER NOT NULL,
    lead_p50_seconds INTEGER,
    lead_p90_seconds INTEGER,
    mttr_seconds INTEGER,
    mttr_restored INTEGER NOT NULL,
    mttr_unrestored INTEGER NOT NULL,
    breach_count INTEGER NOT NULL,
    policy_fingerprint TEXT NOT NULL,
    derivation TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dora_snapshots_rev ON dora_snapshots(source_rev, id);
