`do-harness` turns an agent's task list into gated, evidence-backed work: sensors
run the checks, the evidence artifact records what was verified on which
workspace fingerprint, and the release gate refuses to publish unverified work.

`{{VERSION}}` ships the prebuilt binaries, the three crates.io packages, and —
on Linux and macOS — the npm wrapper. Every command below is pinned to `{{TAG}}`.

## Breaking changes

None. Upgrading from the previous release needs no source or configuration
change.

<!-- Replace this section with the migration steps when that is not true; a
release that breaks a workspace contract says so here, not in the changelog. -->

## Install

```bash
# pinned, SHA-256 verified binary (Linux x86_64/arm64, macOS x86_64/arm64)
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
  | sh -s -- --version {{TAG}}

# from crates.io (builds from source)
cargo install do-harness --version {{VERSION}}

# run without installing (Linux and macOS)
npx do-harness --help
```

Windows ships as `do-harness-{{TAG}}-x86_64-pc-windows-msvc.zip` in this release
only: npm rejects the platform package name, so the npm channel does not exist on
Windows. Unzip it, run the installer under Git Bash, or use `cargo install`.

## Verify the download

```bash
gh release download {{TAG}} --repo d-o-hub/do-harness
sha256sum -c checksums.txt                              # is the download intact?

gh release verify {{TAG}} --repo d-o-hub/do-harness     # is the release immutable?
gh release verify-asset {{TAG}} <asset> --repo d-o-hub/do-harness
gh attestation verify <asset> --repo d-o-hub/do-harness # who built it, from which commit?
```

`checksums.txt` covers every asset in this release and detects a corrupt or
truncated download — but it travels from the same origin as the artifact, so it
proves integrity, not origin. `checksums.txt` and these attestations answer
different questions:

- **GitHub releases** — `gh release verify` checks the attestation GitHub mints
  for an immutable release (tag, commit, and assets), so it also proves the
  release cannot have changed since publication; `gh release verify-asset` checks
  one local file against it, and `gh attestation verify` checks the build
  provenance of the file itself — the workflow, ref, and commit that produced it.
- **npm** — the meta package and every platform package are published from this
  repository's `release.yml` workflow with a provenance attestation;
  `npm audit signatures` reports it for an installed tree.
- **crates.io** — published from the same workflow through the registry's Trusted
  Publishing (OIDC), so the release path holds no long-lived publish token.

## Requirements

- Linux x86_64 or arm64 (static musl), macOS x86_64 or arm64, Windows x86_64.
- `cargo install`: Rust 1.85 or newer.
- npm wrapper: Node 18 or newer.

## Known limitations

`--features mcp-surface` requires Rust 1.88 while the workspace declares 1.85:
the feature is off by default and its MSRV applies only when it is enabled.
