-- 0002_invariants.sql
-- Machine-readable architecture decision headers.

CREATE TABLE IF NOT EXISTS invariants (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    invariant TEXT NOT NULL UNIQUE,
    rationale TEXT NOT NULL,
    sensor TEXT NOT NULL,
    category TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_invariants_category ON invariants(category);