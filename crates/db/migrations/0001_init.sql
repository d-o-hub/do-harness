-- 0001_init.sql
-- Core schema for the do-harness agent execution harness.

-- Execution beats: heartbeat/step tracking for tasks.
CREATE TABLE IF NOT EXISTS beats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
    beat_type TEXT NOT NULL,
    status TEXT NOT NULL,
    sensor_exit_code INTEGER,
    started_at INTEGER NOT NULL,
    completed_at INTEGER
);

-- HTN task decomposition.
CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    method TEXT,
    subtask_index INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    precondition TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Interaction / execution traces for distillation.
CREATE TABLE IF NOT EXISTS traces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER REFERENCES tasks(id) ON DELETE SET NULL,
    session_id TEXT NOT NULL,
    command TEXT,
    error_diff TEXT,
    resolution_steps TEXT,
    created_at INTEGER NOT NULL
);

-- Learned / distilled heuristics.
CREATE TABLE IF NOT EXISTS heuristics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    skill_name TEXT NOT NULL,
    pattern TEXT NOT NULL,
    description TEXT,
    source_trace_id INTEGER REFERENCES traces(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL
);

-- Fail-fast error signatures.
CREATE TABLE IF NOT EXISTS error_signatures (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    signature TEXT NOT NULL,
    task_id INTEGER REFERENCES tasks(id) ON DELETE SET NULL,
    attempt_count INTEGER NOT NULL DEFAULT 1,
    message TEXT,
    created_at INTEGER NOT NULL
);

-- Skill evaluation benchmarks.
CREATE TABLE IF NOT EXISTS skill_evals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    skill_name TEXT NOT NULL,
    prompt TEXT,
    expected_outcome TEXT,
    token_efficiency REAL,
    pass_rate REAL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_beats_task_id ON beats(task_id);
CREATE INDEX IF NOT EXISTS idx_tasks_parent_id ON tasks(parent_id);
CREATE INDEX IF NOT EXISTS idx_traces_session_id ON traces(session_id);
CREATE INDEX IF NOT EXISTS idx_error_signatures_signature ON error_signatures(signature);
CREATE INDEX IF NOT EXISTS idx_skill_evals_skill_name ON skill_evals(skill_name);
