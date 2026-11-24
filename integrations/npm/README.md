# do-harness (npm wrapper)

Zero-install launcher for the [do-harness](../../README.md) CLI:

<!-- example: id=npm_npx_wrapper classification=illustrative reason="npx wrapper command syntax" -->
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

<!-- example: id=npm_install_sh classification=illustrative reason="Single line shell installer invocation" -->
```sh
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh
```

**Windows x64 downloads the release binary on first run.** The
`do-harness-win32-x64` platform package is rejected by the npm registry's
name screening (HTTP 403, "Package name triggered spam detection"), so the
meta package's pinned `optionalDependency` cannot resolve. The shim then
fetches the matching GitHub release zip, verifies its SHA-256 against the
release `checksums.txt`, and caches the extracted `do-harness.exe` under
`%LOCALAPPDATA%\do-harness`; it fails closed on a checksum mismatch or a
missing entry, and there is no `postinstall` script. Disable the fetch with
`DO_HARNESS_NO_DOWNLOAD=1`. See the Windows section of
[docs/adoption.md](../../docs/adoption.md).

Note that `npm install do-harness` **succeeds** on Windows anyway: an absent
optional dependency is not an install error. The binary is fetched at the
first invocation (above); when the fetch is disabled or fails, the shim exits
1 and points at the release zip rather than suggesting `--include=optional`,
which cannot help.

## Notes

- `npx do-harness ...` runs commands without adding anything to `PATH`.
- For git hooks, point `DO_HARNESS_BIN` at the installed wrapper
  (`node_modules/.bin/do-harness`) or install the CLI onto `PATH` with the
  shell installer; managed hooks resolve `$DO_HARNESS_BIN` first.
- Versions are pinned: the meta package depends on exact platform package
  versions, and every publish is generated from a tagged release.
