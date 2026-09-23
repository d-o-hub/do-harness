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
sha256sum -c checksums.txt

# provenance: every asset carries an attestation signed by this repository's
# release workflow, bound to the commit that built it
gh attestation verify <the-asset-you-downloaded> --repo d-o-hub/do-harness
```

`checksums.txt` covers every asset in this release and detects a corrupt or
truncated download — but it travels from the same origin as the artifact, so it
proves integrity, not origin. Each channel publishes its own provenance evidence:

- **GitHub assets** — `gh attestation verify` checks the Sigstore-signed build
  attestation against the workflow, ref, and commit that produced the file.
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
