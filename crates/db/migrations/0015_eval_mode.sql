-- How each skill-eval run executed: deterministic walkthrough residue or an
-- external agent command per case. Lift means different things per mode
-- (artifact dependence vs. real guidance value), so the mode must persist
-- for metrics to stay honest.

ALTER TABLE skill_eval_runs ADD COLUMN mode TEXT NOT NULL DEFAULT 'deterministic';
