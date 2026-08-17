-- 0006_eval_latest.sql
-- Collapse skill_evals history to one row per skill (latest wins) and drop the
-- never-written token_efficiency column. Subsequent writes upsert on the now
-- unique skill_name.

-- Keep only the most recent row per skill.
DELETE FROM skill_evals WHERE id NOT IN (
    SELECT MAX(id) FROM skill_evals GROUP BY skill_name
);

-- Rebuild without token_efficiency and with a UNIQUE skill_name.
CREATE TABLE skill_evals_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    skill_name TEXT NOT NULL UNIQUE,
    prompt TEXT,
    expected_outcome TEXT,
    pass_rate REAL,
    created_at INTEGER NOT NULL
);

INSERT INTO skill_evals_new (id, skill_name, prompt, expected_outcome, pass_rate, created_at)
    SELECT id, skill_name, prompt, expected_outcome, pass_rate, created_at FROM skill_evals;

DROP TABLE skill_evals;
ALTER TABLE skill_evals_new RENAME TO skill_evals;
