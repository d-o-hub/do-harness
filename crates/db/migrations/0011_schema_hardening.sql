-- Schema hardening (#37): make the hash-chain columns mandatory and unique,
-- enforce the sensor-name invariant on beats, and track invariant updates.
--
-- SQLite cannot add NOT NULL/UNIQUE/CHECK to existing columns, so tables with
-- those constraints are rebuilt. Migration 0010 backfilled `seq`/`chain_hash`
-- for every pre-existing row and this migration is only correct when no NULL
-- chain rows remain (true on every known database; the catalog guard in
-- `crates/db/tests/schema_usage.rs` asserts the constraints hold post-migrate).

CREATE TABLE workflow_events_new (
    id INTEGER PRIMARY KEY,
    task_id INTEGER NOT NULL REFERENCES tasks(id),
    kind TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    seq INTEGER NOT NULL UNIQUE,
    chain_hash TEXT NOT NULL UNIQUE
);

INSERT INTO workflow_events_new (id, task_id, kind, payload, created_at, seq, chain_hash)
SELECT id, task_id, kind, payload, created_at, seq, chain_hash FROM workflow_events;

DROP TABLE workflow_events;
ALTER TABLE workflow_events_new RENAME TO workflow_events;
CREATE INDEX IF NOT EXISTS idx_workflow_events_task ON workflow_events(task_id, id);

CREATE TABLE beats_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
    beat_type TEXT NOT NULL,
    status TEXT NOT NULL,
    sensor_exit_code INTEGER,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    sensor_name TEXT,
    CHECK (beat_type != 'sensor' OR sensor_name IS NOT NULL)
);

INSERT INTO beats_new (id, task_id, beat_type, status, sensor_exit_code, started_at, completed_at, sensor_name)
SELECT id, task_id, beat_type, status, sensor_exit_code, started_at, completed_at, sensor_name FROM beats;

DROP TABLE beats;
ALTER TABLE beats_new RENAME TO beats;
CREATE INDEX IF NOT EXISTS idx_beats_task_id ON beats(task_id);

ALTER TABLE invariants ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
UPDATE invariants SET updated_at = created_at;
