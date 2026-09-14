-- 0017_sensor_baselines.sql
-- Findings ratchet telemetry: observed maxima and append-only bless history.
--
-- `sensor_findings` keeps the highest findings count `verify --record` has
-- observed per sensor (monotonic, for trends and metrics). The committed
-- `plans/baselines.json` stays the enforcement source; this table keeps the
-- local observation history. `sensor_baseline_blesses` audits every explicit
-- bless (who lowered which ceiling and when), mirroring skill_eval_blesses.

CREATE TABLE IF NOT EXISTS sensor_findings (
    sensor_name TEXT PRIMARY KEY,
    max_findings INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sensor_baseline_blesses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sensor_name TEXT NOT NULL,
    max_findings INTEGER NOT NULL,
    previous_max INTEGER,
    approver TEXT NOT NULL,
    blessed_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sensor_baseline_blesses_sensor
    ON sensor_baseline_blesses(sensor_name, id);
