# Provenance Verification Trust Model

This document specifies the trust model for `do-harness` artifact and document provenance verification. It outlines the precise claims proven and not proven by each provenance mechanism, delineates source-level auditing from artifact-level verification, and provides the consumer verification model for release artifacts.

---

## 1. Trust-Boundary Diagram

The provenance verification trust model separates source repository management, build execution, signing, artifact distribution, and consumer-side verification into distinct security boundaries:

```
┌────────────────────────────────────────────────────────────────────────┐
│ Boundary 1: Source & Dependency Registries                             │
│  - Git Repository (d-o-hub/do-harness @ git commit)                   │
│  - Upstream Registries (crates.io, npm, cargo lockfiles)              │
└──────────────────────────────────┬─────────────────────────────────────┘
                                   │ git push / tag v*
                                   ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Boundary 2: Build Environment & Runner                                 │
│  - GitHub Actions Runner (ubuntu-latest, macos-14, windows-latest)     │
│  - Rust Toolchain & Build Environment (`cargo build --release`)        │
│  - Metadata Embedder (`cargo-auditable`)                               │
└──────────────────┬────────────────────────────────┬────────────────────┘
                   │ OIDC Token Request             │ Artifact Digest
                   ▼                                ▼
┌──────────────────────────────────────┐  ┌──────────────────────────────┐
│ Boundary 3: Attestation Authority    │  │ Boundary 4: Distribution     │
│  - GitHub OIDC Provider              │  │  - GitHub Releases           │
│  - Sigstore / Rekor Transparency Log │  │  - crates.io / npm           │
└──────────────────┬───────────────────┘  └──────────────┬───────────────┘
                   │ Signed Attestation                  │ Artifact + Hash
                   └──────────────────┬──────────────────┘
                                      │
                                      ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Boundary 5: Consumer Environment & Verification Harness (`do-harness`) │
│  - `check-artifact-provenance.sh` / `artifact-provenance` sensor      │
│  - `gh attestation verify` / `sha256sum`                              │
│  - Consumer Verification Policy (`EXPECTED_DIGEST`, `VERIFY_MODE`)    │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Capabilities and Non-Claims of Provenance Mechanisms

Each mechanism addresses a specific subset of supply-chain security claims. Over-claiming guarantees introduces false sense of security.

### 2.1 `cargo-auditable`

`cargo-auditable` embeds dependency metadata directly into compiled ELF, PE, and Mach-O binaries in a dedicated `.dep-data` section.

* **What it proves:**
  * The exact dependency graph (crate names, versions, and source registries) is recoverable directly from the compiled binary artifact without needing external lockfiles or source code access.
  * Security scanners (e.g., `cargo-audit bin`) can perform post-build vulnerability checks against the binary artifact.
* **What it DOES NOT prove:**
  * **Build Environment Trust:** It does not prove that the compiler (`rustc`), build scripts (`build.rs`), or CI runner were uncompromised.
  * **Source Integrity:** It does not prove that the dependency code or top-level source code was free from malicious logic, intentional backdoors, or memory safety bugs.
  * **Build Reproducibility:** It does not prove deterministic or byte-for-byte reproducible builds by itself.

### 2.2 GitHub Attestations (SLSA Provenance)

GitHub Artifact Attestations leverage GitHub's OIDC identity provider and Sigstore to bind built binaries to their GitHub Actions workflow execution context.

* **What it proves:**
  * **Build Integrity & Origin:** The exact binary artifact matching SHA-256 digest $D$ was produced by a specific workflow run in repository `d-o-hub/do-harness` at git commit $C$, triggered by tag or ref $R$.
  * **Non-Forgeability:** The attestation signature is anchored in Fulcio short-lived certificates and logged to Rekor, preventing post-hoc forgery of build claims.
* **What it DOES NOT prove:**
  * **Code Safety or Vulnerability Absence:** A signed attestation proves *where and how* an artifact was produced, not that the code compiled within that pipeline is safe, bug-free, or uncompromised.
  * **Absence of Pre-Build Compromise:** If an attacker commits malicious code into source control before the workflow triggers, the attestation will faithfully sign the resulting malicious binary.
  * **Runtime Behavior:** It does not govern or constrain process execution once installed on the consumer machine.

### 2.3 Software Bill of Materials (SBOM)

An SBOM provides a formal machine-readable inventory (e.g., CycloneDX or SPDX) of all software components, transitive dependencies, and build tools.

* **What it proves:**
  * Transparency and inventory completeness for component tracking, licensing compliance, and known vulnerability scanning (CVE/GHSA mapping).
* **What it DOES NOT prove:**
  * **Active Prevention:** An SBOM is an informational asset; it does not prevent supply-chain attacks, block unsafe installations, or detect zero-day exploits.
  * **Tamper Prevention:** An unsigned or unverified SBOM can drift from the actual installed binary if not cryptographically bound to the artifact digest.

---

## 3. Source/Dependency Audit vs. Artifact-Level Verification

`do-harness` enforces verification controls at two distinct operational layers:

| Dimension | Source & Dependency Audit | Artifact-Level Verification |
|---|---|---|
| **Primary Scope** | Source code, `Cargo.lock`, `package.json`, policy files (`deny.toml`, `do-harness.toml`). | Built binaries, distribution archives (`.tar.gz`, `.zip`), `checksums.txt`, attestations. |
| **Tooling & Sensors** | `cargo-deny`, `cargo-audit`, `check-deps.sh`, `check-loc.sh`, `commitlint`. | `check-artifact-provenance.sh`, `sha256sum`, `gh attestation verify`. |
| **Verification Point** | Pre-build / Development / Pull-Request CI (`do-harness verify`). | Post-build / Release Distribution / Installation (`install.sh`, `npx`). |
| **Guarantee Offered** | Source code adheres to policy constraints, license rules, and lockfile advisory checks. | The distributed byte stream has not been corrupted or tampered with since pipeline creation. |
| **Key Limitation** | Cannot detect post-checkout build environment tampering or distribution mirror replacement. | Cannot detect source-level logic flaws or backdoors merged prior to build trigger. |

---

## 4. Consumer Verification Model for Release Artifacts

Consumers and automated deployment harnesses verifying `do-harness` release artifacts follow a three-tier verification policy.

```
       [ Download Release Artifact & checksums.txt ]
                            │
                            ▼
               ┌─────────────────────────┐
               │  1. Checksum Match?     ├───────► [ FAIL: Corrupted / Truncated ]
               └────────────┬────────────┘
                            │ Pass
                            ▼
               ┌─────────────────────────┐
               │  2. Attestation Match?  ├───────► [ FAIL: Untrusted Origin ]
               └────────────┬────────────┘
                            │ Pass
                            ▼
               ┌─────────────────────────┐
               │  3. Embedded Dep Audit  ├───────► [ WARN / FAIL: Vulnerability ]
               └────────────┬────────────┘
                            │ Pass
                            ▼
             [ Artifact Cleared for Execution ]
```

### 4.1 Step-by-Step Verification Protocol

1. **Digest Integrity Check (`digest-only` mode):**
   * Compute binary SHA-256 digest: `sha256sum do-harness-vX.Y.Z-<target>.tar.gz`.
   * Compare against official release `checksums.txt`.
   * *Outcome:* Protects against network corruption, incomplete downloads, and mirror modification.

2. **Attestation & Origin Verification (`github-attestation` / `slsa` mode):**
   * Execute:
     ```bash
     gh attestation verify do-harness-vX.Y.Z-<target>.tar.gz \
       --owner d-o-hub
     ```
   * Assert predicate claims:
     * Repository matches `d-o-hub/do-harness`.
     * Workflow identity matches `.github/workflows/release.yml`.
   * *Outcome:* Proves artifact origin without trusting intermediate distribution mirrors.

3. **Embedded Metadata Inspection (`cargo-auditable`):**
   * For extracted binary artifacts, run:
     ```bash
     cargo audit bin path/to/do-harness
     ```
   * *Outcome:* Confirms no embedded dependencies contain newly published high-severity advisories.

### 4.2 Failure Mode Decision Matrix

| Verification Step | Failure Mode | Root Cause Analysis | Action Required |
|---|---|---|
| **Checksum Check** | Mismatched SHA-256 digest | Network corruption, truncated download, or tampered distribution host. | Abort install. Re-download artifact from canonical release location. |
| **Attestation Check** | Signature missing or untrusted signer | Artifact built outside official release workflow, or modified post-build. | Reject artifact. Do not execute binary. |
| **Attestation Check** | Repository / Workflow mismatch | Artifact originated from a fork or untrusted repository. | Reject artifact. Alert security team if expected from main pipeline. |
| **Auditable Scan** | CVE advisory detected in embedded metadata | Upstream dependency vulnerability published post-release. | File issue. Evaluate severity against exposure risk before running. |
