# Webhook fast path (optional)

## Why
Avoids 15–30s polling in `checks.sh --wait` and `post-merge.sh`. A local webhook receiver long-polls `/wait` and wakes on deliveries. Deliveries are hints only: scripts always re-run authoritative GitHub classifications; payloads are never parsed or persisted.

## Setup
1. **Receiver**:
   ```bash
   node .agents/skills/pr-triage/scripts/webhook-receiver.mjs --port 8766
   ```
   Binds `127.0.0.1:8766` (`--allow-remote` only behind proxy/tunnel). Appends delivery summary JSONL to `<git-dir>/pr-triage/events.jsonl`.
2. **Forwarder**:
   ```bash
   gh webhook forward --repo=<owner/repo> \
     --events=pull_request,check_run,workflow_run \
     --url=http://127.0.0.1:8766/events \
     --secret="$GITHUB_WEBHOOK_SECRET"
   ```
3. **Waits**:
   Pass `--events-url http://127.0.0.1:8766` to `scripts/checks.sh` / `scripts/post-merge.sh`, or set `export PR_TRIAGE_EVENTS_URL=http://127.0.0.1:8766`. Missing receiver cleanly falls back to polling.

## Environment
| Variable | Used by | Meaning |
|---|---|---|
| `PR_TRIAGE_EVENTS_URL` | waits | Receiver base URL when `--events-url` omitted |
| `PR_TRIAGE_EVENT_CHUNK` | waits | Safety re-poll chunk seconds (default 300, clamped 5–600) |
| `PR_TRIAGE_WEBHOOK_PORT` | receiver | Port default (8766) |
| `PR_TRIAGE_EVENTS_FILE` | receiver | Summary events file (`<git-dir>/pr-triage/events.jsonl`) |
| `GITHUB_WEBHOOK_SECRET` | receiver | HMAC secret (unsigned rejected with 401 when set) |

## Security & Troubleshooting
- Non-loopback binds require `--allow-remote` and `GITHUB_WEBHOOK_SECRET`.
- Webhook payloads are untrusted: never interpolated into commands or PR comments.
- `falling back to polling`: receiver inactive or curl missing (sweep continues safely).
- Repeated `401`: forwarder `--secret` mismatch.
