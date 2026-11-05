import { test } from "node:test";
import assert from "node:assert/strict";
import { summarize } from "../summary.js";
import { parse } from "../parser.js";

test("summarizes stable counts", () => {
  const records = parse("[INFO] build: started\n[WARN] db: slow query\n[ERROR] db: connection refused\n");
  const report = summarize(records);
  assert.equal(report.total, 3);
  assert.deepEqual(report.by_severity, { ERROR: 1, INFO: 1, WARN: 1 });
});

test("preserves record order in the report", () => {
  const records = parse("[ERROR] db: connection refused\n[WARN] db: slow query\n");
  const report = summarize(records);
  assert.equal(report.records[0].severity, "ERROR");
  assert.equal(report.records[1].severity, "WARN");
});

test("empty input yields zero counts", () => {
  const report = summarize([]);
  assert.equal(report.total, 0);
  assert.deepEqual(report.by_severity, { ERROR: 0, INFO: 0, WARN: 0 });
  assert.deepEqual(report.records, []);
});
