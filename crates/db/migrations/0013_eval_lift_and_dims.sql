-- Skill Lift and evaluation dimensions.
--
-- `skill_eval_runs` gains the without-skill baseline (`without_pass_rate`)
-- plus the context-cost proxies (`skill_words`, `walk_secs`) so token and
-- execution efficiency regressions are visible next to pass rates.
-- `skill_eval_dim_rates` breaks each run into the five NVIDIA-style
-- dimensions so per-dimension lift is measurable, not just the aggregate.

ALTER TABLE skill_eval_runs ADD COLUMN without_pass_rate REAL;
ALTER TABLE skill_eval_runs ADD COLUMN skill_words INTEGER;
ALTER TABLE skill_eval_runs ADD COLUMN walk_secs REAL;

CREATE TABLE IF NOT EXISTS skill_eval_dim_rates (
    run_id INTEGER NOT NULL REFERENCES skill_eval_runs(id) ON DELETE CASCADE,
    dim TEXT NOT NULL,
    graded INTEGER NOT NULL DEFAULT 0,
    passed INTEGER NOT NULL DEFAULT 0,
    without_passed INTEGER,
    PRIMARY KEY (run_id, dim)
);

CREATE INDEX IF NOT EXISTS idx_skill_eval_dim_rates_run
    ON skill_eval_dim_rates(run_id);
