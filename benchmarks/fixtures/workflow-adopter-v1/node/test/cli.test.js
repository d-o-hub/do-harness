import { test } from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const execFileAsync = promisify(execFile);
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cli = join(root, "cli.js");

async function run(args) {
  return execFileAsync(process.execPath, [cli, ...args], { cwd: root });
}

test("filters by component and preserves order", async () => {
  const { stdout } = await run(["--component", "db", "--file", "logs/sample.log"]);
  const report = JSON.parse(stdout);
  assert.ok(report.records.every((record) => record.component === "db"));
  assert.deepEqual(
    report.records.map((record) => record.severity),
    ["ERROR", "INFO", "WARN"],
  );
});

test("minimum severity and message substring filter", async () => {
  const { stdout } = await run(["--min-severity", "WARN", "--message", "query", "--file", "logs/sample.log"]);
  const report = JSON.parse(stdout);
  assert.equal(report.total, 1);
  assert.deepEqual(report.by_severity, { ERROR: 0, INFO: 0, WARN: 1 });
});
test("counts match retained records", async () => {
  const { stdout } = await run(["--min-severity", "ERROR", "--file", "logs/sample.log"]);
  const report = JSON.parse(stdout);
  assert.equal(report.total, report.records.length);
  assert.equal(report.by_severity.ERROR, report.records.length);
});

test("empty input is stable", async () => {
  const { stdout } = await run(["--file", "logs/empty.log"]);
  assert.deepEqual(JSON.parse(stdout), {
    total: 0,
    by_severity: { ERROR: 0, INFO: 0, WARN: 0 },
    records: [],
  });
});

test("unknown option fails with exit 2", async () => {
  await assert.rejects(run(["--bogus"]), (error) => error.code === 2);
});

test("help exits zero", async () => {
  const { stdout } = await run(["--help"]);
  assert.match(stdout, /Usage:/);
});
