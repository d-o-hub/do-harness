# libSQL source-distribution parity (#289)

## Decision

The workspace-local `[patch.crates-io]` did not survive publication. Cargo
normalizes package manifests and removes workspace/patch sections; `--locked`
selects a lockfile, but cannot ship code absent from registry dependencies.

Checked 2026-10-08: stable `libsql` 0.9.30, latest 0.10.0-pre.4 and upstream
`main` still call `sqlite3_close_v2` without clearing the handle in
`local::Connection::disconnect()`. Keep the existing source fix and publish the
vendored crate as `do-harness-libsql` 0.9.30, using a versioned path dependency
aliased as `libsql`. Normalized `do-harness-db` packages name this fork directly.
The fork retains its MIT license, authors, upstream manifest and provenance.

The workspace advances to 0.3.0: `do-harness-db` publicly exposes libSQL types,
whose crate identity changes with the renamed dependency. Rust consumers must
upgrade the do-harness crates together and use `do_harness_db::Connection` or
the same fork alias rather than mixing registry `libsql` types. CLI flags and
database files do not change.

## Executable gates

- `check-package-contract.sh` checks the dependency policy, fork package files
  and negative provenance regression tests.
- `check_source_distribution.py --mode packaged` uses real Cargo archives and
  an isolated directory registry. Its fresh Cargo home and working directory
  exclude checkout patches and ambient Cargo configuration. Every normalized
  dependency is a registry dependency; the CLI is installed from its archive.
- The same script's `--mode registry` installs the exact published CLI with
  `--locked`, inspects the resolved graph and checks the fixed source digest
  after canonicalizing Windows CRLF line endings to LF.
  An additional unpatched `libsql`, a path/git override, a changed source
  digest or a wrong version/source fails even if the process smoke succeeds.
- Both installed and prebuilt CLIs run `smoke_db_distribution.py`: 32 cycles
  of real temporary databases, writes, reopened queries and migration reads.
  Varied path/argv lengths exercise allocation-dependent teardown behavior.
- PR CI validates musl and Windows. The release matrix validates packaged
  installs on every binary target; actual registry validation on musl and
  Windows gates GitHub and npm publication after crates.io publishing.
- Logs contain compiler/LLVM details, target, artifact version, resolved
  dependency source/version, connection digest and subprocess smoke results.

## First-publish record and operator step

The repository's `crates-io-name-check` skill was applied:

<!-- example: id=plans_libsql_first_publish classification=illustrative reason="Terminal output record from crates.io name check" -->
```text
curl (User-Agent: do-harness-name-check; Accept: application/json)
https://crates.io/api/v1/crates/do-harness-libsql
do-harness-libsql HTTP 404
cargo search do-harness-libsql
(no matches; exit 0)
```

This is functional maintained code under the project's prefix, not a reserved
placeholder. Name availability is time-sensitive; recheck before the first
publication. Configure crates.io publication for the new name as described in
`docs/releasing.md` before tagging. No registry package is published by this PR.

Until 0.3.0 has passed actual registry validation, registry source installations
of prior versions still resolve unpatched upstream libSQL. Use tested prebuilt
release artifacts, or build this checkout with its versioned fork dependency.

## Publication record (2026-10-15)

`do-harness-libsql` 0.9.30 was published manually to crates.io on 2026-10-15
(crates.io API: `published_by: d-o-hub`, `trustpub_data: null`) because Trusted
Publishing for the new name was not yet configured. The `v0.3.0` tag exists on
`6804b41`; its publish job failed (`do-harness-libsql 0.9.30 already exists`, the
idempotency check fixed later in #312/`787c19f`) and
`registry-validation`/`npm-publish`/`release` were skipped. Recovery: configure
Trusted Publishing for `do-harness-libsql`, re-cut the tag on `787c19f`, then
complete the registry-validation step above.

## Upstream issue watch

The vendored line is libSQL 0.9.30. Upstream released 0.10.0 on 2026-10-13 and
carries open soundness/durability issues that affect this base version: #2285
(`Row::get_value` truncates embedded NUL text and panics on malformed UTF-8,
reported against 0.9.30), #2271 (bundled SQLite predates the 3.51.3 WAL-reset
fix — silent loss of committed writes under concurrent checkpointing), #2254
(corruption on power loss), and #2272 (`Connection::authorizer` keeps a raw
pointer to the connection, heap use-after-free). The harness uses libSQL as the
persistence engine for all task/beat/trace state, so a 0.10.0 rebase decision is
due.

The watch is `scripts/check-libsql-watch.sh`: it queries the GitHub issues API
for tursodatabase/libsql #2254, #2271, #2272, and #2285 and prints each issue's
state plus a summary of the still-open ones affecting the 0.9.30 base
(`VERDICT=OPEN|CLEAR|UNKNOWN`, always exit 0 — a fetch or parse failure says so
rather than reporting a clean line). It mirrors `scripts/check-agt-ga.sh` and,
like it, is **not** a `do-harness verify` sensor because it needs the network.
Its hermetic parse contract is pinned by `crates/do-harness/tests/libsql_watch.rs`
over `tests/fixtures/libsql-watch/*.json`, run with
`scripts/check-libsql-watch.sh --fixtures <file>`. Record the affected issues here
as they land.

## References

- <https://doc.rust-lang.org/cargo/commands/cargo-package.html>
- <https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html>
- <https://docs.rs/crate/libsql/0.9.30/source/src/local/connection.rs>
- <https://docs.rs/crate/libsql/0.10.0-pre.4/source/src/local/connection.rs>
- <https://github.com/tursodatabase/libsql/blob/main/libsql/src/local/connection.rs>
