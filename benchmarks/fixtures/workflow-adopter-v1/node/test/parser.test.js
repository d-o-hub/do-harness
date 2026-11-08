import { test } from "node:test";
import assert from "node:assert/strict";
import { parse, parseLine, severityRank } from "../parser.js";

test("parses a record with component and message", () => {
  assert.deepEqual(parseLine("[ERROR] db: connection refused"), {
    severity: "ERROR",
    component: "db",
    message: "connection refused",
  });
});

test("skips unrecognized lines", () => {
  assert.equal(parseLine("not a record"), null);
  assert.equal(parseLine("[UNKNOWN] x: y"), null);
  assert.equal(parseLine("[INFO] : missing component"), null);
  assert.equal(parseLine("[INFO] missing colon"), null);
});

test("tolerates surrounding whitespace", () => {
  assert.deepEqual(parseLine("  [WARN]   cache : miss  "), {
    severity: "WARN",
    component: "cache",
    message: "miss",
  });
});

test("parses multiple records in order", () => {
  const records = parse("[ERROR] db: connection refused\n[WARN] db: slow query\n");
  assert.equal(records.length, 2);
  assert.equal(records[0].severity, "ERROR");
  assert.equal(records[1].severity, "WARN");
});

test("severity ranks are ordered", () => {
  // The rank table is a Map, so an unknown severity can never index a
  // prototype slot; the ordering assertion reads it the same way the filter does.
  assert.ok(severityRank.get("INFO") < severityRank.get("WARN"));
  assert.ok(severityRank.get("WARN") < severityRank.get("ERROR"));
  assert.equal(severityRank.get("VERBOSE"), undefined);
});
