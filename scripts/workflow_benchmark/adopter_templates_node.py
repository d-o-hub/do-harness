"""Pinned adopter fixture templates: the Node `pinned-api-router` project.

This is the documented offline equivalent of a TypeScript pack: the module
surface is checked against `types/router.d.ts` by `scripts/build-check.js`.
Regenerating reproduces the committed tree byte-for-byte.
"""

NODE_PACKAGE_JSON = r"""
{
  "name": "pinned-api-router",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "build": "node scripts/build-check.js",
    "test": "node tests/run.js"
  }
}
"""

NODE_ROUTER = r"""
// Synthetic pinned adopter fixture: a small dependency-free API router.
// The module surface is checked against types/router.d.ts by build-check.js.
import { validate } from "./validate.js";
import { Cache } from "./cache.js";

export class Router {
  constructor(routes) {
    this.routes = routes ?? [];
    this.cache = new Cache();
  }

  dispatch(pathname, body, options = {}) {
    const route = this.routes.find((r) => r.pattern === pathname);
    if (!route) {
      return { status: 404, body: { error: "not_found" } };
    }
    const { valid, errors } = validate(route.input, body);
    if (!valid) {
      return { status: 422, body: { error: "invalid_input", fields: errors } };
    }
    // Baseline gap: query parameters are accepted but never forwarded to the
    // handler, and validation never normalizes whitespace-only strings.
    const cached = this.cache.get(pathname, body, {});
    if (cached) {
      return { status: 200, body: cached, fromCache: true };
    }
    const result = route.handler(body);
    this.cache.set(pathname, body, {}, result);
    return { status: 200, body: result, fromCache: false };
  }
}
"""

NODE_VALIDATE = r"""
// Field validation for routed inputs (part of the multi-file fix surface).
const TYPES = new Set(["string", "number", "boolean"]);

export function validate(schema, value) {
  const errors = [];
  if (typeof value !== "object" || value === null) {
    return { valid: false, errors: ["body must be an object"] };
  }
  for (const [key, rule] of Object.entries(schema ?? {})) {
    if (rule.required && value[key] === undefined) {
      errors.push(`${key} is required`);
      continue;
    }
    if (value[key] === undefined) continue;
    if (rule.type && !TYPES.has(rule.type)) {
      errors.push(`${key} has unknown type ${rule.type}`);
      continue;
    }
    if (rule.type === "string" && typeof value[key] !== "string") {
      errors.push(`${key} must be a string`);
    } else if (rule.type === "number" && typeof value[key] !== "number") {
      errors.push(`${key} must be a number`);
    }
  }
  return { valid: errors.length === 0, errors };
}

export function normalize(value) {
  if (typeof value === "string") {
    return value.trim();
  }
  return value;
}
"""

NODE_CACHE = r"""
// Small fixed-capacity cache; part of the edit-after-green surface.
const DEFAULT_MAX_ENTRIES = 128;

export class Cache {
  constructor(maxEntries = DEFAULT_MAX_ENTRIES) {
    this.maxEntries = maxEntries;
    this.map = new Map();
  }

  key(pathname, body, query) {
    return JSON.stringify([pathname, body, query]);
  }

  get(pathname, body, query) {
    const hit = this.map.get(this.key(pathname, body, query));
    if (hit) {
      this.map.delete(this.key(pathname, body, query));
      this.map.set(this.key(pathname, body, query), hit);
    }
    return hit ?? null;
  }

  set(pathname, body, query, result) {
    const key = this.key(pathname, body, query);
    this.map.set(key, result);
    if (this.map.size > this.maxEntries) {
      this.map.delete(this.map.keys().next().value);
    }
  }
}
"""

NODE_TYPES_DTS = r"""
// Declaration surface checked by scripts/build-check.js (TS-equivalent).
export interface RouteInput {
  [field: string]: { type?: "string" | "number" | "boolean"; required?: boolean };
}

export interface Route {
  pattern: string;
  input: RouteInput;
  handler(body: unknown, query: Record<string, string>): unknown;
}

export interface DispatchResult {
  status: number;
  body: unknown;
  fromCache?: boolean;
}

export declare class Router {
  constructor(routes?: Route[]);
  dispatch(pathname: string, body: unknown, options?: {
    query?: Record<string, string>;
  }): DispatchResult;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
}

export declare function validate(
  schema: RouteInput,
  value: unknown
): ValidationResult;

export declare function normalize(value: unknown): unknown;

export declare class Cache {
  constructor(maxEntries?: number);
  get(pathname: string, body: unknown, query: Record<string, string>): unknown;
  set(
    pathname: string,
    body: unknown,
    query: Record<string, string>,
    result: unknown
  ): void;
}
"""

NODE_BUILD_CHECK = r"""
// Build check: syntax-check every source module and verify the exported
// surface matches types/router.d.ts. This is the offline, dependency-free
// equivalent of a TypeScript build step for this synthetic fixture.
import { readFileSync, readdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

for (const name of ["src/router.js", "src/validate.js", "src/cache.js"]) {
  try {
    execFileSync(process.execPath, ["--check", join(root, name)], {
      stdio: "pipe",
    });
  } catch {
    process.exitCode = 1;
    console.error(`build-check: syntax error in ${name}`);
    process.exit(1);
  }
}

const declared = ["Router", "validate", "normalize", "Cache"];
const actual = new Set();
for (const file of readdirSync(join(root, "src"))) {
  const source = readFileSync(join(root, "src", file), "utf8");
  for (const line of source.split("\n")) {
    const match = line.match(/^export (?:class|function) ([A-Za-z0-9_]+)/);
    if (match) actual.add(match[1]);
  }
}
for (const name of declared) {
  if (!actual.has(name)) {
    console.error(`build-check: missing export ${name}`);
    process.exit(1);
  }
}
console.log("build-check: ok");
"""

NODE_TESTS = r"""
// Test runner for the adopter corpus. Prints one JSON object with a boolean
// for each case target plus the shared regression result.
import { Router } from "../src/router.js";
import { normalize } from "../src/validate.js";
import { Cache } from "../src/cache.js";

const routes = [
  {
    pattern: "/users",
    input: { name: { type: "string", required: true } },
    handler(body, query = {}) {
      return { id: 7, name: normalize(String(body.name)), q: query.q ?? null };
    },
  },
];

function run(label, fn) {
  try {
    return fn();
  } catch (error) {
    console.error(`suite ${label} crashed: ${error}`);
    return false;
  }
}

const regression = run("regression", () => {
  const router = new Router(routes);
  const missing = router.dispatch("/users", {});
  if (missing.status !== 422) return false;
  const unknown = router.dispatch("/nope", {});
  if (unknown.status !== 404) return false;
  const hit = router.dispatch("/users", { name: "ada" });
  const cached = router.dispatch("/users", { name: "ada" });
  return hit.status === 200 && cached.fromCache === true && hit.body.id === 7;
});

const multifile = run("multifile", () => {
  const router = new Router(routes);
  const withQuery = router.dispatch("/users", { name: "  ada  " }, { query: { q: "x" } });
  if (withQuery.status !== 200) return false;
  if (withQuery.body.name !== "ada") return false; // normalized
  if (withQuery.body.q !== "x") return false; // query forwarded to handler
  // Validation must reject whitespace-only required strings.
  const blank = router.dispatch("/users", { name: "   " });
  if (blank.status !== 422) return false;
  return true;
});

const editAfterGreen = run("editAfterGreen", () => {
  const cache = new Cache(2);
  const key = cache.key("/a", { n: 1 }, {});
  cache.set("/a", { n: 1 }, {}, "one");
  cache.set("/b", { n: 2 }, {}, "two");
  cache.set("/c", { n: 3 }, {}, "three");
  if (cache.get("/a", { n: 1 }, {}) !== null) return false; // evicted
  if (cache.get("/b", { n: 2 }, {}) !== "two") return false;
  if (cache.get("/c", { n: 3 }, {}) !== "three") return false;
  return key === cache.key("/a", { n: 1 }, {});
});

console.log(
  JSON.stringify({
    regression,
    multifile,
    editAfterGreen,
  })
);
"""


# ---------------------------------------------------------------------------
# Fixed variants: the achievable correct end-states used as corpus patches and
# by self-tests.
# ---------------------------------------------------------------------------

NODE_ROUTER_FIXED = r"""
// Synthetic pinned adopter fixture: a small dependency-free API router.
// The module surface is checked against types/router.d.ts by build-check.js.
import { validate } from "./validate.js";
import { Cache } from "./cache.js";

export class Router {
  constructor(routes) {
    this.routes = routes ?? [];
    this.cache = new Cache();
  }

  dispatch(pathname, body, options = {}) {
    const route = this.routes.find((r) => r.pattern === pathname);
    if (!route) {
      return { status: 404, body: { error: "not_found" } };
    }
    const { valid, errors } = validate(route.input, body);
    if (!valid) {
      return { status: 422, body: { error: "invalid_input", fields: errors } };
    }
    const query = options.query ?? {};
    const cached = this.cache.get(pathname, body, query);
    if (cached) {
      return { status: 200, body: cached, fromCache: true };
    }
    const result = route.handler(body, query);
    this.cache.set(pathname, body, query, result);
    return { status: 200, body: result, fromCache: false };
  }
}
"""

NODE_VALIDATE_FIXED = r"""
// Field validation for routed inputs. Strings are trimmed before use and
// whitespace-only required strings are rejected.
const TYPES = new Set(["string", "number", "boolean"]);

export function validate(schema, value) {
  const errors = [];
  if (typeof value !== "object" || value === null) {
    return { valid: false, errors: ["body must be an object"] };
  }
  for (const [key, rule] of Object.entries(schema ?? {})) {
    if (rule.required && value[key] === undefined) {
      errors.push(`${key} is required`);
      continue;
    }
    if (value[key] === undefined) continue;
    if (rule.type && !TYPES.has(rule.type)) {
      errors.push(`${key} has unknown type ${rule.type}`);
      continue;
    }
    if (rule.type === "string" && typeof value[key] !== "string") {
      errors.push(`${key} must be a string`);
    } else if (rule.type === "string" && value[key].trim() === "") {
      errors.push(`${key} must not be blank`);
    } else if (rule.type === "number" && typeof value[key] !== "number") {
      errors.push(`${key} must be a number`);
    }
  }
  return { valid: errors.length === 0, errors };
}

export function normalize(value) {
  if (typeof value === "string") {
    return value.trim();
  }
  return value;
}
"""
