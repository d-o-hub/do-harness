# do-harness (npm wrapper)

Zero-install launcher for the [do-harness](../../README.md) CLI:

```sh
npx do-harness init
npx do-harness verify --set verification --format json --strict
npx do-harness status --set verification
```

The wrapper contains no verification policy. It resolves the prebuilt binary
for the current platform from a platform package installed through
`optionalDependencies` and execs it with inherited stdio.

Supported platforms: Linux x64/arm64 (static musl), macOS x64/arm64. Other
platforms install from source or download a release; Linux/macOS can use the
shell installer:

```sh
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh
```

**Windows x64 is not available through npm.** The `do-harness-win32-x64`
platform package is rejected by the npm registry's name screening (HTTP 403,
"Package name triggered spam detection"), so the meta package's pinned
`optionalDependency` cannot resolve and `npx do-harness` has no Windows binary
to run. The Windows binary ships as a GitHub release zip instead — see the
Windows section of [docs/adoption.md](../../docs/adoption.md).

Note that `npm install do-harness` **succeeds** on Windows anyway: an absent
optional dependency is not an install error. The failure appears at run time,
where the shim exits 1 and points at the release zip rather than suggesting
`--include=optional`, which cannot help.

## Notes

- `npx do-harness ...` runs commands without adding anything to `PATH`.
- For git hooks, point `DO_HARNESS_BIN` at the installed wrapper
  (`node_modules/.bin/do-harness`) or install the CLI onto `PATH` with the
  shell installer; managed hooks resolve `$DO_HARNESS_BIN` first.
- Versions are pinned: the meta package depends on exact platform package
  versions, and every publish is generated from a tagged release.
