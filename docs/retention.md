# Log and artifact retention

| Artifact | Location | Retention | Mechanism |
|---|---|---|---|
| Workflow events / beats | `.do-harness/agent_state.db` | Local; prune beats older than 30 days while keeping ≥20 per task | `do-harness maintenance --prune-beats 30 --keep-per-task 20` |
| Skill-eval history | `.do-harness/agent_state.db` (`skill_eval_runs`, `skill_eval_blesses`) | Full history retained locally (small); bless history is never pruned because it is the approval audit trail | `maintenance` prunes beats only |
| Proxy audit log | `ProxyConfig.audit_log` (e.g. `guardian-audit.jsonl`) | Operator-defined; append-only hash chain, `fsync` per append | rotate by copying and starting a new file; `--verify-audit` checks a file at any time |
| CI evidence | `.do-harness/evidence.json` artifact | GitHub Actions artifact retention (default 90 days), one artifact per run | `actions/upload-artifact` in `.github/workflows/verify.yml` |
| Cargo.lock hash | `.do-harness/Cargo.lock.sha256` artifact | Same as CI evidence | Uploaded alongside the evidence artifact |
| Interaction traces | `.do-harness/agent_state.db` (`traces`) | Local, retained in full — `trace` only adds, lists, or lists sessions | no clear/prune command yet; `maintenance --prune-beats` prunes beats only |

The local database holds no CI secrets; CI uploads only the evidence artifact
and the lockfile hash, never the database.

## Version control

The state database is local and never committed; `.do-harness/` is ignored
(`git check-ignore -v .do-harness/agent_state.db` matches `.gitignore`). Three
reasons, all of them from the tools' own documentation:

- A live SQLite file is not safely copyable. A copy taken while a transaction
  is in flight "might contain some old and some new content, and thus be
  corrupt", and any copy that must be consistent needs the `-journal`/`-wal`
  file alongside it — <https://sqlite.org/howtocorrupt.html> §1.2. `git add`
  is such a copy, and an open database is accompanied by
  `agent_state.db-wal`/`-shm` sidecars that git would either miss or commit as
  meaningless transients.
- It cannot be diffed or merged, so every `verify --record` leaves the tree
  dirty, every run stores a fresh full blob, and any two branches conflict
  irreconcilably.
- Hosting is explicit: "Git is not designed to handle large SQL files. To share
  large databases with other developers, we recommend using a file sharing
  service." — <https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github>.

It would also publish `beats`, `error_signatures`, and `traces` content
(sensor output tails, commands, diffs, absolute paths) into history forever.

Version the contract, not the cache:

- schema — `crates/db/migrations/*.sql`, applied by `connect_and_migrate`;
- gates — `plans/baselines.json`, `plans/invariants.json`, `plans/dora.json`,
  `plans/methods.json`;
- snapshots — `do-harness task export` (`plans/tasks.json`),
  `metrics --format json`, `sqlite3 .dump`;
- shared state that must outlive a clone — a text export, a CI artifact, a
  release asset, or a server-backed database. Never a live file in git, and
  never Git LFS for a mutable database: LFS versions each rewrite as a new
  blob and cannot merge.
