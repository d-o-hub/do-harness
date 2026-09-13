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

Supported platforms: Linux x64/arm64 (static musl) and macOS x64/arm64.
Windows is not shipped. On unsupported platforms use the shell installer or
build from source:

```sh
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh
```

## Notes

- `npx do-harness ...` runs commands without adding anything to `PATH`.
- For git hooks, point `DO_HARNESS_BIN` at the installed wrapper
  (`node_modules/.bin/do-harness`) or install the CLI onto `PATH` with the
  shell installer; managed hooks resolve `$DO_HARNESS_BIN` first.
- Versions are pinned: the meta package depends on exact platform package
  versions, and every publish is generated from a tagged release.
