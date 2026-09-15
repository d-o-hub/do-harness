# Webhook fast path (optional)

## Why

`checks.sh --wait` and `post-merge.sh` otherwise poll every 30 s / 15 s, so a
finished run is noticed up to a poll interval late and every poll spends API
calls. With a local webhook receiver the wait long-polls `/wait` and wakes on
the delivery, usually sub-second, while a safety chunk re-classifies if
deliveries are dropped.

Deliveries are **hints only**. After every wake the scripts re-run the
authoritative GitHub classification, and the receiver returns no payload data.
A lost or hostile delivery can make a wait slower, never wrong.

## Setup

1. **Receiver** (any machine running the sweep):

   ```bash
   node .agents/skills/pr-triage/scripts/webhook-receiver.mjs --port 8766
   ```

   It prints `listening http://127.0.0.1:8766` once bound. Run it as a
   background process (human: `nohup ... &`; agent harness: start it with the
   process manager and match the `listening` line). It binds loopback only
   unless `--allow-remote` is passed, and appends one summary JSON line per
   accepted delivery to `<git-dir>/pr-triage/events.jsonl` (override with
   `--events` or `PR_TRIAGE_EVENTS_FILE`). Payloads are never persisted.

2. **Forwarder** — either of these:

   ```bash
   gh extension install cli/gh-webhook   # once
   gh webhook forward --repo=<owner/repo> \
     --events=pull_request,check_run,workflow_run \
     --url=http://127.0.0.1:8766/events \
     --secret="$GITHUB_WEBHOOK_SECRET"
   ```

   (`gh webhook forward` requires `--events`, one of `--repo`/`--org`, and
   forwards GitHub's headers to `--url`; `--secret` sets the temporary dev
   hook's secret so the deliveries are signed.)

   Or point a repository webhook at a tunnel (smee, ngrok) that forwards to
   `http://127.0.0.1:8766/events`.

3. **Waits** — pass the URL, or export it once for the sweep:

   ```bash
   scripts/checks.sh 123 --wait 1200 --events-url http://127.0.0.1:8766
   scripts/post-merge.sh 123 --events-url http://127.0.0.1:8766
   export PR_TRIAGE_EVENTS_URL=http://127.0.0.1:8766
   ```

   The scripts print `event wait armed (...)`, `event wake (...)` on a wake,
   and `event wait unavailable; falling back to polling` when the receiver
   (or `curl`) is missing. Exit codes and summaries are identical either way.

## Environment

| Variable | Used by | Meaning |
|----------|---------|---------|
| `PR_TRIAGE_EVENTS_URL` | waits | Receiver base URL when `--events-url` is absent |
| `PR_TRIAGE_EVENT_CHUNK` | waits | Safety chunk seconds, default 300, clamped 5–600 |
| `PR_TRIAGE_WEBHOOK_PORT` | receiver | Port default when `--port` is absent (8766) |
| `PR_TRIAGE_EVENTS_FILE` | receiver | Events file when `--events` is absent (`<git-dir>/pr-triage/events.jsonl`) |
| `GITHUB_WEBHOOK_SECRET` | receiver | HMAC secret; unset means unsigned deliveries are accepted |

## Security

- Loopback bind by default; `--allow-remote` is required for any other host,
  and only makes sense when a tunnel or reverse proxy terminates the request.
- Set `GITHUB_WEBHOOK_SECRET` (and the matching `--secret` on the forwarder)
  whenever anything but loopback can reach the receiver: deliveries without a
  valid `X-Hub-Signature-256` are rejected with `401` and never wake a wait.
- Webhook bodies are untrusted data. The scripts never extract values from
  payloads and never interpolate them into commands; a lying endpoint can cost
  an extra `gh api` call, not a wrong verdict.
- Never paste payload contents into a command line or a PR comment.

## Troubleshooting

- `falling back to polling` → receiver not running, URL wrong, or `curl`
  missing; the sweep continues with normal polling.
- Repeated `401` → `GITHUB_WEBHOOK_SECRET` and the forwarder's `--secret`
  differ, or the hook has no secret while the receiver requires one.
- Port busy → `--port <other>` and matching `--events-url`.
- No wakes but checks finish → deliveries are being dropped; the safety chunk
  still re-classifies and `tail <git-dir>/pr-triage/events.jsonl` shows what
  arrived (summaries only, no payloads).
