`do-harness` turns an agent's task list into gated, evidence-backed work: sensors
run the checks, the evidence artifact records what was verified on which
workspace fingerprint, and the release gate refuses to publish unverified work.
`{{VERSION}}` is the patch release described below.

### Install

```bash
# pinned, SHA-256 verified binary (Linux x86_64/arm64, macOS x86_64/arm64)
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
  | sh -s -- --version {{TAG}}

# from crates.io
cargo install do-harness --version {{VERSION}}

# or run without installing (Linux/macOS via npm)
npx do-harness --help
```

Windows ships as `do-harness-{{TAG}}-x86_64-pc-windows-msvc.zip` from this
release only: npm rejects the platform package name, so the npm channel does not
exist on Windows (`npx` prints that guidance instead of failing silently).

### Verify the download

```bash
gh release download {{TAG}} --repo d-o-hub/do-harness
sha256sum -c checksums.txt
```

`checksums.txt` covers every artifact in this release. The crates.io packages
carry the registry's own attestations, and the npm packages are published with
provenance from this repository's release workflow.

### Known limitations

`--features mcp-surface` requires Rust 1.88 while the workspace declares 1.85:
the feature is off by default and its MSRV applies only when it is enabled.

