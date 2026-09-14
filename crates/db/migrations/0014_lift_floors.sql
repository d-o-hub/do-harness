-- Skill Lift floor: the ratchet for guidance value.
--
-- Mirrors `skill_bars` (pass-rate floor) for the without-skill baseline
-- delta. Only raisable through an explicit bless after a fully green,
-- human-reviewed eval; eval fails below the floor even when the current
-- assertions are green, so silent guidance erosion is caught. Lift ranges
-- [-1, 1]: a negative floor still catches a skill that starts harming runs.

CREATE TABLE IF NOT EXISTS skill_lift_floors (
    skill_name TEXT PRIMARY KEY,
    floor REAL NOT NULL,
    updated_at INTEGER NOT NULL
);
