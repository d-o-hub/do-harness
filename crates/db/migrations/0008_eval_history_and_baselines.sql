-- 0008_eval_history_and_baselines.sql
-- Recursive self-improvement integrity: measure improvement over rounds,
-- raise the bar honestly, and make the graders tamper-evident.

-- Append-only history of every skill evaluation run. The collapsed
-- `skill_evals` view stays the "latest" read model; this table keeps the
-- trend so improvement across rounds is measurable.
CREATE TABLE IF NOT EXISTS skill_eval_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    skill_name TEXT NOT NULL,
    graded INTEGER NOT NULL DEFAULT 0,
    passed INTEGER NOT NULL DEFAULT 0,
    pass_rate REAL,
    ran_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_skill_eval_runs_skill ON skill_eval_runs(skill_name);

-- Per-skill pass-rate floor. Only raisable through an explicit bless after a
-- fully green, human-reviewed eval; eval fails below the floor even when the
-- current assertions are green (the ratchet).
CREATE TABLE IF NOT EXISTS skill_bars (
    skill_name TEXT PRIMARY KEY,
    floor REAL NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Tamper-evidence for the graders themselves: SHA-256 of the walkthrough
-- script and the evals fixture at bless time. Drift fails the eval until a
-- human reviews and re-blesses.
CREATE TABLE IF NOT EXISTS grader_baselines (
    skill_name TEXT PRIMARY KEY,
    walkthrough_sha TEXT NOT NULL,
    specs_sha TEXT NOT NULL,
    blessed_at INTEGER NOT NULL
);
