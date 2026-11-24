// Deterministic JSON documents in the shapes the CLI emits; the TS SDK
// tests never need a real harness binary.

export const VERIFY_OK = {
  ok: true,
  root: "/repo",
  failed: [],
  unrunnable: [],
  sensors: [
    {
      details: { reason: null, output_bytes: 3, output_truncated: false },
      name: "fake-check",
      ok: true,
      exit_code: 0,
      duration_ms: 17,
      severity: "error",
      allow_failure: false,
      warned: false,
      execution: "ran",
    },
  ],
};

export const VERIFY_FAIL = {
  ok: false,
  root: "/repo",
  failed: ["fails"],
  unrunnable: [],
  sensors: [
    {
      details: { reason: null, output_bytes: 5, output_truncated: false },
      name: "fails",
      ok: false,
      exit_code: 1,
      duration_ms: 19,
      severity: "error",
      allow_failure: false,
      warned: false,
      execution: "ran",
    },
    {
      details: { reason: null, output_bytes: 4, output_truncated: false },
      name: "warns",
      ok: false,
      exit_code: 1,
      duration_ms: 22,
      severity: "warn",
      allow_failure: true,
      warned: false,
      execution: "ran",
    },
  ],
};

export const STATUS_GREEN = {
  state: "green",
  set: "verification",
  reason: "",
  current: {
    workspace_fingerprint: "sha256:aa",
    policy_fingerprint: "sha256:bb",
    failed: [],
    input_mismatches: [],
  },
  recovery: null,
};

export const STATUS_RED = {
  state: "red",
  set: "verification",
  reason: "sensor_failed",
  current: {
    workspace_fingerprint: "sha256:aa",
    policy_fingerprint: "sha256:bb",
    failed: ["fails"],
    input_mismatches: [],
  },
  recovery: "do-harness verify --set verification --record",
};

export const EXPLAIN = {
  set: null,
  selection_mode: "all",
  changed_files: [],
  selected: [{ name: "fake-check", reason: "change filtering disabled; always applicable" }],
  skipped: [],
};

export const ERRORS = [
  {
    id: 1,
    signature: "sensor:clippy",
    scope: "branch:main",
    task_id: null,
    attempt_count: 2,
    message: "FAIL: clippy failed",
    created_at: 1795277534,
  },
  {
    id: 2,
    signature: "sensor:test",
    scope: "task:7",
    task_id: 7,
    attempt_count: 1,
    message: null,
    created_at: 1795277535,
  },
];

export const TASK_LIST = {
  summary: { done: 0, failed: 0, in_progress: 0, pending: 1 },
  tasks: [
    {
      created_at: 1795277534,
      id: 1,
      method: null,
      parent_id: null,
      precondition: null,
      status: "pending",
      subtask_index: 0,
      title: "demo task",
      updated_at: 1795277534,
    },
  ],
};

export const TASK_EXPORT = {
  exported_at: 1795277534,
  tasks: TASK_LIST.tasks,
  summary: TASK_LIST.summary,
};

export const VERSION = {
  name: "do-harness",
  version: "0.4.8",
  commit: "4f31a1a",
  commit_date: "2026-11-21",
  dirty: false,
};
