-- Append-only bless history: who approved each grader baseline and when.
--
-- `grader_baselines` stays latest-wins for fast drift checks; this table keeps
-- every approval so a mistaken or coerced bless can be audited after the fact.

CREATE TABLE IF NOT EXISTS skill_eval_blesses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    skill_name TEXT NOT NULL,
    walkthrough_sha TEXT NOT NULL,
    specs_sha TEXT NOT NULL,
    approver TEXT NOT NULL,
    reason TEXT,
    blessed_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_skill_eval_blesses_skill
    ON skill_eval_blesses(skill_name, id);
