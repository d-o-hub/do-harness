import { test } from "node:test";
import assert from "node:assert/strict";
import { apply } from "../filter.js";
import { parse } from "../parser.js";

function fixture() {
  return parse("[INFO] build: started\n[WARN] db: slow query\n[ERROR] db: connection refused\n");
}

test("retains all records by default", () => {
  assert.equal(apply(fixture()).length, 3);
});

test("minimum severity excludes lower severities", () => {
  const retained = apply(fixture(), { min: "ERROR" });
  assert.equal(retained.length, 1);
  assert.equal(retained[0].severity, "ERROR");
});

test("component filter matches exactly", () => {
  const retained = apply(fixture(), { component: "db" });
  assert.ok(retained.every((record) => record.component === "db"));
});

test("message substring is case-insensitive", () => {
  const retained = apply(fixture(), { message: "REFUSED" });
  assert.equal(retained.length, 1);
  assert.equal(retained[0].message, "connection refused");
});

test("filtering preserves record order", () => {
  const retained = apply(fixture(), { component: "db" });
  assert.deepEqual(
    retained.map((record) => record.severity),
    ["WARN", "ERROR"],
  );
});
