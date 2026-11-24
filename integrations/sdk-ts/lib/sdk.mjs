// do-harness-sdk: typed bindings for the do-harness CLI.
//
// Zero-dependency ESM implementation. It spawns the harness binary with an
// argv array (never a shell), asks for JSON output, and parses it into typed
// models. Fail-closed: a missing binary, unparseable (or empty) output, or a
// non-zero exit raises a HarnessError subclass instead of returning a partial
// result. The package version mirrors the CLI release it targets.

import { spawnSync } from "node:child_process";

export const SDK_VERSION = "0.4.8";
export const SCHEMA_VERSION = 1;

export class HarnessError extends Error {
  constructor(message, { exitCode = null, stderr = "", result = null, argv = null } = {}) {
    super(message);
    this.name = new.target.name;
    this.exitCode = exitCode;
    this.stderr = stderr;
    this.result = result;
    this.argv = argv;
  }
}

export class BinaryNotFoundError extends HarnessError {}
export class ParseError extends HarnessError {}
export class CliError extends HarnessError {}

// A `verify` run that reported failing sensors (exit 1); `result` is the
// parsed VerifyResult.
export class VerifyFailed extends CliError {}

// A `status` run with a non-green verdict (exit 1); `result` is the parsed
// StatusResult.
export class StatusNotGreen extends CliError {}

function required(obj, key) {
  if (obj === null || typeof obj !== "object") {
    throw new TypeError("expected a JSON object");
  }
  if (!(key in obj)) {
    throw new Error(`missing required field ${JSON.stringify(key)}`);
  }
  return obj[key];
}

function optional(obj, key, fallback = null) {
  return obj !== null && typeof obj === "object" && key in obj ? obj[key] : fallback;
}

function list(obj, key, parse) {
  const value = optional(obj, key, []);
  if (!Array.isArray(value)) {
    throw new Error(`field ${JSON.stringify(key)} must be an array`);
  }
  return value.map(parse);
}

// -- models ---------------------------------------------------------------

export function parseSensorResult(obj) {
  const details = optional(obj, "details", {});
  return {
    name: String(required(obj, "name")),
    ok: Boolean(required(obj, "ok")),
    exitCode: optional(obj, "exit_code"),
    durationMs: optional(obj, "duration_ms"),
    severity: String(optional(obj, "severity", "error")),
    allowFailure: Boolean(optional(obj, "allow_failure", false)),
    warned: Boolean(optional(obj, "warned", false)),
    execution: String(optional(obj, "execution", "ran")),
    reason: optional(details, "reason"),
    outputBytes: optional(details, "output_bytes"),
    outputTruncated: Boolean(optional(details, "output_truncated", false)),
  };
}

export function parseVerifyResult(obj) {
  return {
    ok: Boolean(required(obj, "ok")),
    root: String(required(obj, "root")),
    failed: list(obj, "failed", String),
    unrunnable: list(obj, "unrunnable", String),
    sensors: list(obj, "sensors", parseSensorResult),
  };
}

export function parseStatusResult(obj) {
  const state = optional(obj, "current");
  return {
    state: String(required(obj, "state")),
    set: optional(obj, "set"),
    reason: String(optional(obj, "reason", "")),
    current:
      state === null
        ? null
        : {
            workspaceFingerprint: String(required(state, "workspace_fingerprint")),
            policyFingerprint: String(required(state, "policy_fingerprint")),
            failed: list(state, "failed", String),
            inputMismatches: list(state, "input_mismatches", String),
          },
    recovery: optional(obj, "recovery"),
  };
}

export function parseExplainResult(obj) {
  return {
    set: optional(obj, "set"),
    selectionMode: String(optional(obj, "selection_mode", "all")),
    changedFiles: list(obj, "changed_files", String),
    selected: list(obj, "selected", (s) => ({
      name: String(required(s, "name")),
      reason: String(optional(s, "reason", "")),
    })),
    skipped: list(obj, "skipped", (s) => ({
      name: String(required(s, "name")),
      reason: String(optional(s, "reason", "")),
    })),
    discoveryError: optional(obj, "discovery_error"),
    since: optional(obj, "since"),
    baseSha: optional(obj, "base_sha"),
    headSha: optional(obj, "head_sha"),
    mergeBaseSha: optional(obj, "merge_base_sha"),
  };
}

export function parseStrikeInfo(obj) {
  return {
    id: Number(required(obj, "id")),
    signature: String(required(obj, "signature")),
    scope: String(required(obj, "scope")),
    attemptCount: Number(required(obj, "attempt_count")),
    taskId: optional(obj, "task_id"),
    message: optional(obj, "message"),
    createdAt: optional(obj, "created_at"),
  };
}

function parseTaskRecord(obj) {
  return {
    id: Number(required(obj, "id")),
    title: String(required(obj, "title")),
    status: String(required(obj, "status")),
    parentId: optional(obj, "parent_id"),
    method: optional(obj, "method"),
    subtaskIndex: Number(optional(obj, "subtask_index", 0)),
    precondition: optional(obj, "precondition"),
    createdAt: optional(obj, "created_at"),
    updatedAt: optional(obj, "updated_at"),
  };
}

function parseTaskSummary(obj) {
  return {
    pending: Number(optional(obj, "pending", 0)),
    inProgress: Number(optional(obj, "in_progress", 0)),
    done: Number(optional(obj, "done", 0)),
    failed: Number(optional(obj, "failed", 0)),
  };
}

export function parseTaskList(obj) {
  return {
    summary: parseTaskSummary(required(obj, "summary")),
    tasks: list(obj, "tasks", parseTaskRecord),
  };
}

export function parseTaskExport(obj) {
  return {
    exportedAt: Number(required(obj, "exported_at")),
    summary: parseTaskSummary(required(obj, "summary")),
    tasks: list(obj, "tasks", parseTaskRecord),
  };
}

export function parseCliVersion(obj) {
  return {
    name: String(required(obj, "name")),
    version: String(required(obj, "version")),
    commit: optional(obj, "commit"),
    commitDate: optional(obj, "commit_date"),
    dirty: Boolean(optional(obj, "dirty", false)),
  };
}

// -- client ---------------------------------------------------------------

export class HarnessClient {
  constructor({ binary = undefined, root = null } = {}) {
    this.binary = binary ?? process.env.DO_HARNESS_BIN ?? "do-harness";
    this.root = root;
  }

  _spawn(args) {
    const argv = [this.binary, ...args];
    if (this.root !== null) {
      argv.push("--root", this.root);
    }
    const res = spawnSync(argv[0], argv.slice(1), { encoding: "utf8" });
    if (res.error) {
      throw new BinaryNotFoundError(`do-harness binary not found: ${argv[0]}`, {
        stderr: String(res.error),
        argv,
      });
    }
    let payload = null;
    const out = (res.stdout ?? "").trim();
    if (out) {
      try {
        payload = JSON.parse(out);
      } catch (err) {
        throw new ParseError(
          `do-harness emitted unparseable output (exit ${res.status}): ${err.message}`,
          { exitCode: res.status, stderr: res.stderr, argv },
        );
      }
    } else if (res.status === 0) {
      throw new ParseError("do-harness produced no output", {
        exitCode: 0,
        stderr: res.stderr,
        argv,
      });
    }
    if (res.status !== 0) {
      throw new CliError(`do-harness exited ${res.status}`, {
        exitCode: res.status,
        stderr: res.stderr,
        result: payload,
        argv,
      });
    }
    return payload;
  }

  verify(options = {}) {
    const args = ["verify", "--format", "json"];
    if (options.signalSet) args.push("--set", options.signalSet);
    if (options.strict) args.push("--strict");
    if (options.changed) args.push("--changed");
    if (options.since) args.push("--since", options.since);
    if (options.failFast) args.push("--fail-fast");
    for (const name of options.only ?? []) args.push("--only", name);
    for (const name of options.exclude ?? []) args.push("--exclude", name);
    if (options.jobs !== undefined) args.push("--jobs", String(options.jobs));
    if (options.extra) args.push(...options.extra);
    try {
      return parseVerifyResult(this._spawn(args));
    } catch (err) {
      if (err instanceof CliError && err.exitCode === 1 && err.result && typeof err.result === "object") {
        throw new VerifyFailed("verify reported failing sensors", {
          exitCode: err.exitCode,
          stderr: err.stderr,
          result: parseVerifyResult(err.result),
          argv: err.argv,
        });
      }
      throw err;
    }
  }

  status(options = {}) {
    const args = ["status", "--format", "json"];
    if (options.signalSet) args.push("--set", options.signalSet);
    if (options.evidence) args.push("--evidence", options.evidence);
    if (options.since) args.push("--since", options.since);
    if (options.extra) args.push(...options.extra);
    try {
      return parseStatusResult(this._spawn(args));
    } catch (err) {
      if (err instanceof CliError && err.exitCode === 1 && err.result && typeof err.result === "object") {
        throw new StatusNotGreen("status is not green", {
          exitCode: err.exitCode,
          stderr: err.stderr,
          result: parseStatusResult(err.result),
          argv: err.argv,
        });
      }
      throw err;
    }
  }

  explain(options = {}) {
    const args = ["explain", "--format", "json"];
    if (options.signalSet) args.push("--set", options.signalSet);
    if (options.changed) args.push("--changed");
    if (options.since) args.push("--since", options.since);
    if (options.extra) args.push(...options.extra);
    return parseExplainResult(this._spawn(args));
  }

  errorsList(options = {}) {
    const args = ["errors", "list", "--format", "json"];
    if (options.taskId) args.push("--task", options.taskId);
    if (options.scope) args.push("--scope", options.scope);
    if (options.extra) args.push(...options.extra);
    const payload = this._spawn(args);
    if (!Array.isArray(payload)) {
      throw new ParseError("errors list document must be an array", { result: payload });
    }
    return payload.map(parseStrikeInfo);
  }

  taskList(options = {}) {
    const args = ["task", "list", "--format", "json"];
    if (options.status) args.push("--status", options.status);
    if (options.method) args.push("--method", options.method);
    if (options.parent) args.push("--parent", options.parent);
    if (options.extra) args.push(...options.extra);
    return parseTaskList(this._spawn(args));
  }

  taskExport(options = {}) {
    const args = ["task", "export", "--stdout", "--format", "json"];
    if (options.extra) args.push(...options.extra);
    return parseTaskExport(this._spawn(args));
  }

  version() {
    return parseCliVersion(this._spawn(["version", "--format", "json"]));
  }

  assertSchemaCompatible() {
    const cli = this.version();
    const sdkMajorMinor = SDK_VERSION.split(".").slice(0, 2).join(".");
    const cliMajorMinor = cli.version.split(".").slice(0, 2).join(".");
    if (cliMajorMinor !== sdkMajorMinor) {
      throw new CliError(
        `SDK ${SDK_VERSION} targets do-harness ${sdkMajorMinor}.x; installed CLI reports ${cli.version}`,
        { result: cli },
      );
    }
    return cli;
  }
}
