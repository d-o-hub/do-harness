# Log and artifact retention

| Artifact | Location | Retention | Mechanism |
|---|---|---|---|
| Workflow events / beats | `.do-harness/agent_state.db` | Local; prune beats older than 30 days while keeping ≥20 per task | `do-harness maintenance --prune-beats 30 --keep-per-task 20` |
| Skill-eval history | `.do-harness/agent_state.db` (`skill_eval_runs`, `skill_eval_blesses`) | Full history retained locally (small); bless history is never pruned because it is the approval audit trail | `maintenance` prunes beats only |
| Proxy audit log | `ProxyConfig.audit_log` (e.g. `guardian-audit.jsonl`) | Operator-defined; append-only hash chain, `fsync` per append | rotate by copying and starting a new file; `--verify-audit` checks a file at any time |
| CI evidence | `.do-harness/evidence.json` artifact | GitHub Actions artifact retention (default 90 days), one artifact per run | `actions/upload-artifact` in `.github/workflows/verify.yml` |
| Cargo.lock hash | `.do-harness/Cargo.lock.sha256` artifact | Same as CI evidence | Uploaded alongside the evidence artifact |
| Interaction traces | `.do-harness/agent_state.db` (`traces`) | Local; clear per session with `do-harness trace` if needed | no automatic prune yet |

The local database holds no CI secrets; CI uploads only the evidence artifact
and the lockfile hash, never the database.
