import test from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  BinaryNotFoundError,
  CliError,
  HarnessClient,
  ParseError,
  SDK_VERSION,
  SCHEMA_VERSION,
  StatusNotGreen,
  VerifyFailed,
} from "../lib/sdk.mjs";

import * as F from "./fixtures.mjs";

const STUB_TEMPLATE = `#!/usr/bin/env node
import fs from "node:fs";
const scenario = process.env.FAKE_SCENARIO ?? "ok";
const spy = process.env.FAKE_SPY_FILE;
if (spy) fs.writeFileSync(spy, JSON.stringify(process.argv.slice(2)));
const print = (v) => process.stdout.write(JSON.stringify(v) + "\\n");
if (scenario === "garbage") {
  process.stdout.write("this is not json\\n");
  process.exit(0);
}
if (scenario === "empty") {
  process.exit(0);
}
if (scenario === "usage-error") {
  process.stderr.write("error: unknown signal set 'nope'\\n");
  process.exit(2);
}
if (scenario === "verify-fail") {
  print(@VERIFY_FAIL@);
  process.exit(1);
}
if (scenario === "status-red") {
  print(@STATUS_RED@);
  process.exit(1);
}
if (scenario === "exit2-with-report") {
  print(@VERIFY_FAIL@);
  process.exit(2);
}
if (scenario === "version-mismatch") {
  const bad = structuredClone(@VERSION@);
  bad.version = "0.5.0";
  print(bad);
  process.exit(0);
}
const cmd = process.argv.slice(2);
if (cmd[0] === "verify") print(@VERIFY_OK@);
else if (cmd[0] === "status") print(@STATUS_GREEN@);
else if (cmd[0] === "explain") print(@EXPLAIN@);
else if (cmd[0] === "errors" && cmd[1] === "list") print(@ERRORS@);
else if (cmd[0] === "task" && cmd[1] === "list") print(@TASK_LIST@);
else if (cmd[0] === "task" && cmd[1] === "export") print(@TASK_EXPORT@);
else if (cmd[0] === "version") print(@VERSION@);
else {
  process.stderr.write("error: unexpected command " + JSON.stringify(cmd) + "\\n");
  process.exit(2);
}
`;

function buildStub() {
  let script = STUB_TEMPLATE;
  const payloads = {
    "@VERIFY_OK@": F.VERIFY_OK,
    "@VERIFY_FAIL@": F.VERIFY_FAIL,
    "@STATUS_GREEN@": F.STATUS_GREEN,
    "@STATUS_RED@": F.STATUS_RED,
    "@EXPLAIN@": F.EXPLAIN,
    "@ERRORS@": F.ERRORS,
    "@TASK_LIST@": F.TASK_LIST,
    "@TASK_EXPORT@": F.TASK_EXPORT,
    "@VERSION@": F.VERSION,
  };
  for (const [token, payload] of Object.entries(payloads)) {
    script = script.split(token).join(JSON.stringify(payload));
  }
  const dir = mkdtempSync(join(tmpdir(), "dh-sdk-"));
  const stubPath = join(dir, "fake-do-harness");
  writeFileSync(stubPath, script);
  chmodSync(stubPath, 0o755);
  return stubPath;
}

const STUB = buildStub();

function client(options = {}) {
  return new HarnessClient({ binary: STUB, ...options });
}

function withScenario(scenario, fn) {
  const prev = process.env.FAKE_SCENARIO;
  if (scenario === null) delete process.env.FAKE_SCENARIO;
  else process.env.FAKE_SCENARIO = scenario;
  try {
    return fn();
  } finally {
    if (prev === undefined) delete process.env.FAKE_SCENARIO;
    else process.env.FAKE_SCENARIO = prev;
  }
}

function withSpy(fn) {
  const dir = mkdtempSync(join(tmpdir(), "dh-spy-"));
  const spyPath = join(dir, "argv.json");
  process.env.FAKE_SPY_FILE = spyPath;
  try {
    const result = fn();
    const argv = JSON.parse(readFileSync(spyPath, "utf8"));
    return { result, argv };
  } finally {
    delete process.env.FAKE_SPY_FILE;
  }
}

test("version constant mirrors CLI release", () => {
  assert.equal(SDK_VERSION, "0.4.8");
  assert.equal(SCHEMA_VERSION, 1);
});

test("verify parses ok and passes argv array", () => {
  const { result, argv } = withSpy(() => client().verify({ signalSet: "verification" }));
  assert.equal(result.ok, true);
  assert.equal(result.root, "/repo");
  assert.deepEqual(result.failed, []);
  assert.deepEqual(result.unrunnable, []);
  assert.equal(result.sensors.length, 1);
  assert.equal(result.sensors[0].name, "fake-check");
  assert.equal(result.sensors[0].exitCode, 0);
  assert.equal(result.sensors[0].execution, "ran");
  assert.deepEqual(argv, ["verify", "--format", "json", "--set", "verification"]);
});

test("verify passes extra flags", () => {
  const { argv } = withSpy(() =>
    client().verify({ strict: true, changed: true, since: "origin/main", failFast: true }),
  );
  assert.deepEqual(argv, [
    "verify",
    "--format",
    "json",
    "--strict",
    "--changed",
    "--since",
    "origin/main",
    "--fail-fast",
  ]);
});

test("verify failure raises VerifyFailed carrying report", () => {
  withScenario("verify-fail", () => {
    assert.throws(
      () => client().verify(),
      (err) => {
        assert.ok(err instanceof VerifyFailed);
        assert.equal(err.exitCode, 1);
        assert.equal(err.result.ok, false);
        assert.deepEqual(err.result.failed, ["fails"]);
        assert.deepEqual(err.result.sensors.map((s) => s.name), ["fails", "warns"]);
        return true;
      },
    );
  });
});

test("status parses green", () => {
  const result = client().status({ signalSet: "verification" });
  assert.equal(result.state, "green");
  assert.equal(result.current.workspaceFingerprint, "sha256:aa");
});

test("status red raises StatusNotGreen carrying report", () => {
  withScenario("status-red", () => {
    assert.throws(
      () => client().status(),
      (err) => {
        assert.ok(err instanceof StatusNotGreen);
        assert.equal(err.result.state, "red");
        assert.equal(err.result.reason, "sensor_failed");
        assert.deepEqual(err.result.current.failed, ["fails"]);
        assert.equal(err.result.recovery, "do-harness verify --set verification --record");
        return true;
      },
    );
  });
});

test("explain parses", () => {
  const result = client().explain();
  assert.equal(result.selectionMode, "all");
  assert.equal(result.selected[0].name, "fake-check");
  assert.match(result.selected[0].reason, /always applicable/);
});

test("errors list parses strikes", () => {
  const strikes = client().errorsList({ scope: "branch:main" });
  assert.equal(strikes.length, 2);
  assert.equal(strikes[0].signature, "sensor:clippy");
  assert.equal(strikes[0].attemptCount, 2);
  assert.equal(strikes[0].taskId, null);
  assert.equal(strikes[1].taskId, 7);
});

test("task list and export parse", () => {
  const board = client().taskList({ status: "pending" });
  assert.equal(board.summary.pending, 1);
  assert.equal(board.tasks[0].title, "demo task");
  const snapshot = client().taskExport();
  assert.equal(snapshot.exportedAt, 1795277534);
  assert.deepEqual(snapshot.tasks, board.tasks);
});

test("version parses", () => {
  const info = client().version();
  assert.equal(info.name, "do-harness");
  assert.equal(info.version, "0.4.8");
  assert.equal(info.dirty, false);
});

test("assertSchemaCompatible matches", () => {
  assert.equal(client().assertSchemaCompatible().version, "0.4.8");
});

test("assertSchemaCompatible fails on mismatch", () => {
  withScenario("version-mismatch", () => {
    assert.throws(
      () => client().assertSchemaCompatible(),
      (err) => {
        assert.ok(err instanceof CliError);
        assert.match(err.message, /0.5.0/);
        return true;
      },
    );
  });
});

test("root flag is passed", () => {
  const { argv } = withSpy(() => client({ root: "/srv/repo" }).status());
  assert.deepEqual(argv, ["status", "--format", "json", "--root", "/srv/repo"]);
});

test("missing binary raises BinaryNotFoundError", () => {
  assert.throws(() => new HarnessClient({ binary: "/nonexistent/do-harness" }).version(), BinaryNotFoundError);
});

test("unparseable output raises ParseError", () => {
  withScenario("garbage", () => {
    assert.throws(() => client().verify(), ParseError);
    assert.throws(() => client().status(), ParseError);
  });
});

test("empty output raises ParseError", () => {
  withScenario("empty", () => {
    assert.throws(() => client().verify(), (err) => {
      assert.ok(err instanceof ParseError);
      assert.match(err.message, /no output/);
      return true;
    });
  });
});

test("usage error exit 2 raises CliError with stderr", () => {
  withScenario("usage-error", () => {
    assert.throws(
      () => client().explain({ signalSet: "nope" }),
      (err) => {
        assert.ok(err instanceof CliError);
        assert.equal(err.exitCode, 2);
        assert.match(err.stderr, /unknown signal set/);
        return true;
      },
    );
  });
});

test("exit 2 with report raises CliError carrying result", () => {
  withScenario("exit2-with-report", () => {
    assert.throws(
      () => client().verify(),
      (err) => {
        assert.ok(err instanceof CliError);
        assert.equal(err.exitCode, 2);
        assert.equal(err.result.ok, false);
        return true;
      },
    );
  });
});

test("extra arguments are appended", () => {
  const { argv } = withSpy(() => client().explain({ extra: ["--since", "HEAD~3"] }));
  assert.deepEqual(argv, ["explain", "--format", "json", "--since", "HEAD~3"]);
});
