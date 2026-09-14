-- Bars and lift floors are per eval mode: deterministic lift measures
-- artifact dependence while agent lift measures guidance value, so a single
-- floor cannot honestly judge both. Rebuild both tables with a
-- (skill_name, mode) primary key; existing rows become deterministic floors.

CREATE TABLE skill_bars_new (
    skill_name TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'deterministic',
    floor REAL NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (skill_name, mode)
);

INSERT INTO skill_bars_new (skill_name, mode, floor, updated_at)
SELECT skill_name, 'deterministic', floor, updated_at FROM skill_bars;

DROP TABLE skill_bars;
ALTER TABLE skill_bars_new RENAME TO skill_bars;

CREATE TABLE skill_lift_floors_new (
    skill_name TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'deterministic',
    floor REAL NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (skill_name, mode)
);

INSERT INTO skill_lift_floors_new (skill_name, mode, floor, updated_at)
SELECT skill_name, 'deterministic', floor, updated_at FROM skill_lift_floors;

DROP TABLE skill_lift_floors;
ALTER TABLE skill_lift_floors_new RENAME TO skill_lift_floors;
