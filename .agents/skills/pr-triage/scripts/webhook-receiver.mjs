#!/usr/bin/env node
// Local GitHub webhook receiver for the pr-triage fast path.
//
// Zero-dependency sidecar (Node >= 18): GitHub — or `gh webhook forward` —
// POSTs deliveries here; each one is summarised into one JSON line appended
// to the events file and pushed into a bounded in-memory ring, and the
// pr-triage wait scripts long-poll GET /wait instead of sleeping. Deliveries
// are hints only: the wait scripts re-run the authoritative GitHub
// classification after every wake, so a dropped or hostile delivery can
// delay a verdict but never corrupt one.
//
// Usage:
//   webhook-receiver.mjs [--port <n>] [--host <h>] [--events <path>]
//                        [--secret <value>] [--allow-remote]
//
// Defaults: port 8766 (or PR_TRIAGE_WEBHOOK_PORT), host 127.0.0.1,
// events from PR_TRIAGE_EVENTS_FILE or <git-dir>/pr-triage/events.jsonl
// (same git-dir resolution as state.sh), secret from GITHUB_WEBHOOK_SECRET
// (no secret -> signatures not required).
// Exit: 0 on SIGINT/SIGTERM, 1 on bind or write failure, 2 on usage error.
//
// Routes:
//   GET  /health                              -> 200 "ok"
//   GET  /wait?since=<n>&timeout=<1..600>     -> 200 {"cursor":<n>,"woke":<bool>}
//        (&events=<csv>; default check_run,workflow_run)
//   POST <any path>                           -> 202 {"ok":true} | 4xx
//
// The receiver never logs or persists the secret or the signature, and never
// persists raw payloads: one summary JSON object per line, values as data.

import { execFileSync } from "node:child_process";
import { createHmac, timingSafeEqual } from "node:crypto";
import { appendFileSync, closeSync, mkdirSync, openSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, join } from "node:path";

const MAX_BODY_BYTES = 1024 * 1024;
const RING_CAP = 1000;
const DEFAULT_EVENTS = ["check_run", "workflow_run"];
const DEFAULT_TIMEOUT_SECS = 30;
const MAX_TIMEOUT_SECS = 600;

function usage(message) {
  if (message) {
    console.error(message);
  }
  console.error(
    "usage: webhook-receiver.mjs [--port <n>] [--host <h>] [--events <path>] [--secret <value>] [--allow-remote]",
  );
  process.exit(2);
}

let portOpt = null;
let host = "127.0.0.1";
let eventsOpt = null;
let secretOpt = null;
let allowRemote = false;

const argv = process.argv.slice(2);
for (let i = 0; i < argv.length; i += 1) {
  const flag = argv[i];
  const value = () => {
    i += 1;
    if (i >= argv.length) {
      usage(`webhook-receiver: ${flag} needs a value`);
    }
    return argv[i];
  };
  switch (flag) {
    case "--port":
      portOpt = value();
      break;
    case "--host":
      host = value();
      break;
    case "--events":
      eventsOpt = value();
      break;
    case "--secret":
      secretOpt = value();
      break;
    case "--allow-remote":
      allowRemote = true;
      break;
    case "--help":
      usage();
      break;
    default:
      usage(`webhook-receiver: unknown flag ${flag}`);
  }
}

const portRaw = portOpt ?? process.env.PR_TRIAGE_WEBHOOK_PORT ?? "8766";
if (!/^\d+$/.test(portRaw) || Number(portRaw) > 65535) {
  usage(`webhook-receiver: invalid port '${portRaw}'`);
}
const port = Number(portRaw);

if (host !== "127.0.0.1" && !allowRemote) {
  console.error(`webhook-receiver: refusing to bind ${host} without --allow-remote`);
  process.exit(2);
}

let eventsPath = eventsOpt ?? process.env.PR_TRIAGE_EVENTS_FILE ?? null;
if (eventsPath === null || eventsPath === "") {
  let gitDir = null;
  try {
    gitDir = execFileSync("git", ["rev-parse", "--absolute-git-dir"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    gitDir = null;
  }
  if (!gitDir) {
    console.error("webhook-receiver: not a git repository; pass --events <path>");
    process.exit(2);
  }
  eventsPath = join(gitDir, "pr-triage", "events.jsonl");
}

const secret = secretOpt || process.env.GITHUB_WEBHOOK_SECRET || "";

try {
  mkdirSync(dirname(eventsPath), { recursive: true });
  closeSync(openSync(eventsPath, "a"));
} catch (err) {
  console.error(`webhook-receiver: cannot open events file ${eventsPath}: ${err.message}`);
  process.exit(1);
}

// In-memory ring of {index, event}; `received` is the count of accepted
// deliveries and doubles as the cursor returned to /wait clients (indexes are
// 0-based, so `since=<received>` means "only events after my last wake").
const ring = [];
let received = 0;
const waiters = new Set();

function summarize(event, payload) {
  const checkRun = payload?.check_run ?? {};
  const workflowRun = payload?.workflow_run ?? {};
  const pullRequest = payload?.pull_request ?? {};
  return {
    received_at: new Date().toISOString(),
    event,
    action: typeof payload?.action === "string" ? payload.action : null,
    sha:
      checkRun?.check_suite?.head_sha ??
      checkRun?.head_sha ??
      workflowRun?.head_sha ??
      pullRequest?.head?.sha ??
      null,
    pr:
      checkRun?.check_suite?.pull_requests?.[0]?.number ??
      workflowRun?.pull_requests?.[0]?.number ??
      pullRequest?.number ??
      null,
    run_id: checkRun?.id ?? workflowRun?.id ?? null,
    conclusion: checkRun?.conclusion ?? workflowRun?.conclusion ?? null,
    workflow: workflowRun?.name ?? checkRun?.name ?? null,
  };
}

function send(res, status, body, type = "text/plain; charset=utf-8") {
  if (res.writableEnded) {
    return;
  }
  res.writeHead(status, { "content-type": type });
  res.end(body);
}

function signatureValid(provided, raw) {
  if (!secret) {
    return true;
  }
  if (typeof provided !== "string") {
    return false;
  }
  const match = /^sha256=([0-9a-fA-F]{64})$/.exec(provided.trim());
  if (!match) {
    return false;
  }
  const expected = createHmac("sha256", secret).update(raw).digest();
  const given = Buffer.from(match[1], "hex");
  return given.length === expected.length && timingSafeEqual(given, expected);
}

function handlePost(req, res) {
  const chunks = [];
  let size = 0;
  let settled = false;

  req.on("data", (chunk) => {
    if (settled) {
      return;
    }
    size += chunk.length;
    if (size > MAX_BODY_BYTES) {
      settled = true;
      send(res, 413, "payload too large");
      return;
    }
    chunks.push(chunk);
  });

  req.on("error", () => {
    settled = true;
  });

  req.on("end", () => {
    if (settled) {
      return;
    }
    settled = true;

    const raw = Buffer.concat(chunks);
    if (!signatureValid(req.headers["x-hub-signature-256"], raw)) {
      send(res, 401, "invalid or missing signature");
      return;
    }
    const event = req.headers["x-github-event"];
    if (typeof event !== "string" || event === "") {
      send(res, 400, "missing X-GitHub-Event header");
      return;
    }
    let payload;
    try {
      payload = JSON.parse(raw.toString("utf8"));
    } catch {
      send(res, 400, "body is not JSON");
      return;
    }

    const record = summarize(event, payload);
    try {
      appendFileSync(eventsPath, `${JSON.stringify(record)}\n`);
    } catch (err) {
      console.error(`webhook-receiver: cannot append to ${eventsPath}: ${err.message}`);
      send(res, 500, "cannot persist event");
      return;
    }

    const index = received;
    received += 1;
    ring.push({ index, event });
    if (ring.length > RING_CAP) {
      ring.shift();
    }
    wakeWaiters(event, index);
    send(res, 202, '{"ok":true}', "application/json");
  });
}

function wakeWaiters(event, index) {
  for (const waiter of [...waiters]) {
    if (waiter.since <= index && waiter.filter.has(event)) {
      waiters.delete(waiter);
      clearTimeout(waiter.timer);
      send(waiter.res, 200, `{"cursor":${received},"woke":true}`, "application/json");
    }
  }
}

function handleWait(url, res) {
  const sinceRaw = Number.parseInt(url.searchParams.get("since") ?? "0", 10);
  const requestedSince = Number.isFinite(sinceRaw) && sinceRaw >= 0 ? sinceRaw : 0;
  // A cursor ahead of this instance's counter means the receiver restarted
  // (indexes are per-process), so it cannot name a delivery any more: treat it
  // as "everything from now on" instead of stalling the client until its old
  // index is reached again.
  const since = Math.min(requestedSince, received);
  const timeoutRaw = Number.parseInt(url.searchParams.get("timeout") ?? "", 10);
  const timeout = Number.isFinite(timeoutRaw)
    ? Math.min(MAX_TIMEOUT_SECS, Math.max(1, timeoutRaw))
    : DEFAULT_TIMEOUT_SECS;
  const requested = (url.searchParams.get("events") ?? "")
    .split(",")
    .map((name) => name.trim())
    .filter((name) => name !== "");
  const filter = new Set(requested.length > 0 ? requested : DEFAULT_EVENTS);

  const already = ring.some((entry) => entry.index >= since && filter.has(entry.event));
  if (already) {
    send(res, 200, `{"cursor":${received},"woke":true}`, "application/json");
    return;
  }

  const waiter = { since, filter, res, timer: null };
  waiter.timer = setTimeout(() => {
    waiters.delete(waiter);
    send(res, 200, `{"cursor":${received},"woke":false}`, "application/json");
  }, timeout * 1000);
  waiters.add(waiter);
  res.on("close", () => {
    waiters.delete(waiter);
    clearTimeout(waiter.timer);
  });
}

const server = createServer((req, res) => {
  const url = new URL(req.url ?? "/", "http://localhost");
  if (req.method === "GET" && url.pathname === "/health") {
    send(res, 200, "ok");
    return;
  }
  if (req.method === "GET" && url.pathname === "/wait") {
    handleWait(url, res);
    return;
  }
  if (req.method === "POST") {
    handlePost(req, res);
    return;
  }
  send(res, 404, "not found");
});

server.on("error", (err) => {
  console.error(`webhook-receiver: ${err.message}`);
  process.exit(1);
});

server.listen(port, host, () => {
  const address = server.address();
  const bound = typeof address === "object" && address !== null ? address.port : port;
  console.log(`listening http://${host}:${bound}`);
});

function shutdown() {
  server.close(() => process.exit(0));
  if (typeof server.closeAllConnections === "function") {
    server.closeAllConnections();
  }
  setTimeout(() => process.exit(0), 500).unref();
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
