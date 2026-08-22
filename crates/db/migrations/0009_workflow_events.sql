-- Append-only workflow event log for the task domain.
--
-- Every workflow command (add/advance/done/fail) persists the emitted
-- WorkflowEvent here in the same transaction as its tasks-row mutation, so
-- read models (the task board) fold a real event stream instead of
-- reconstructing events from current row state.

CREATE TABLE workflow_events (
    id INTEGER PRIMARY KEY,
    task_id INTEGER NOT NULL REFERENCES tasks(id),
    kind TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_workflow_events_task ON workflow_events(task_id, id);

-- Backfill one synthetic event per pre-existing task so history that predates
-- this table folds into the same projections; payloads mirror the serde
-- representation of `do_harness_types::WorkflowEvent` (internally tagged by
-- `kind`).
INSERT INTO workflow_events (task_id, kind, payload, created_at)
SELECT t.id,
       CASE t.status
           WHEN 'pending' THEN 'TaskAdded'
           WHEN 'in_progress' THEN 'TaskAdvanced'
           WHEN 'done' THEN 'TaskCompleted'
           ELSE 'TaskFailed'
       END,
       CASE t.status
           WHEN 'in_progress' THEN json_object(
               'kind', 'TaskAdvanced',
               'id', t.id,
               'subtask_index', t.subtask_index)
           WHEN 'done' THEN json_object('kind', 'TaskCompleted', 'id', t.id)
           WHEN 'failed' THEN json_object('kind', 'TaskFailed', 'id', t.id)
           ELSE json_object(
               'kind', 'TaskAdded',
               'id', t.id,
               'title', t.title,
               'method', t.method)
       END,
       t.updated_at
FROM tasks t;
