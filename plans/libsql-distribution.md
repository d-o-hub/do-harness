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

## References

- <https://doc.rust-lang.org/cargo/commands/cargo-package.html>
- <https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html>
- <https://docs.rs/crate/libsql/0.9.30/source/src/local/connection.rs>
- <https://docs.rs/crate/libsql/0.10.0-pre.4/source/src/local/connection.rs>
- <https://github.com/tursodatabase/libsql/blob/main/libsql/src/local/connection.rs>
