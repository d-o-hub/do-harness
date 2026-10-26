//! Canonical verification inputs shared by verify, status, reuse and task gates.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::SensorSpec;

/// Change this identifier whenever the canonical input contract changes.
pub const ALGORITHM: &str = "sha256-canonical-inputs-v1";

const POLICY_INPUTS: &[&str] = &[
    "do-harness.toml",
    "plans/invariants.json",
    "plans/methods.json",
    "Cargo.lock",
    "rust-toolchain",
    "rust-toolchain.toml",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "uv.lock",
    "poetry.lock",
    "go.sum",
];
const TOOL_ENV: &[&str] = &[
    "PATH",
    "RUSTUP_TOOLCHAIN",
    "RUSTFLAGS",
    "RUSTC",
    "RUSTDOC",
    "CARGO_BUILD_TARGET",
    "CARGO_ENCODED_RUSTFLAGS",
];

/// Safe input manifest. Values are identities, never input contents or env values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputManifest {
    /// Fingerprint algorithm and contract version.
    pub algorithm: String,
    /// Git revision (None for a repository without commits).
    pub git_revision: Option<String>,
    /// Branch identity, including detached HEAD.
    pub branch: Option<String>,
    /// Per-sensor canonical identity; unknown inputs are explicit nulls.
    pub sensors: BTreeMap<String, Option<String>>,
    /// Declared file/coverage/environment input names, without values.
    pub declarations: BTreeMap<String, Vec<String>>,
    /// Resolved tool content identities; no command is run to infer a pass.
    pub tool_identities: BTreeMap<String, Option<String>>,
}

impl InputManifest {
    /// Take one snapshot using exactly the same identity as recorded task beats.
    pub fn capture(root: &Path, specs: &[&SensorSpec], config: Option<&[u8]>) -> Self {
        let tools = snapshot_tools(root, specs);
        Self {
            algorithm: ALGORITHM.to_owned(),
            git_revision: git_revision(root),
            branch: crate::changes::current_branch(root),
            tool_identities: tools
                .iter()
                .map(|(program, tool)| {
                    (
                        program.clone(),
                        tool.as_ref().map(|(_, digest)| digest.clone()),
                    )
                })
                .collect(),
            sensors: specs
                .iter()
                .map(|s| {
                    (
                        s.name.clone(),
                        sensor_digest_with_tools(root, s, config, &tools),
                    )
                })
                .collect(),
            declarations: specs
                .iter()
                .map(|s| {
                    let mut names: Vec<String> =
                        s.inputs.iter().map(|p| format!("input:{p}")).collect();
                    if names.is_empty() {
                        names.push("input:git-index-and-nonignored-untracked".into());
                    }
                    names.extend(s.coverage_inputs.iter().map(|p| format!("coverage:{p}")));
                    names.extend(
                        s.environment_inputs
                            .iter()
                            .map(|p| format!("environment:{p}")),
                    );
                    names.extend(POLICY_INPUTS.iter().map(|p| format!("policy:{p}")));
                    names.push("tool:resolved-executable-sha256".into());
                    (s.name.clone(), names)
                })
                .collect(),
        }
    }

    /// Unknown or empty manifests cannot establish freshness.
    pub fn established(&self) -> bool {
        self.algorithm == ALGORITHM
            && !self.sensors.is_empty()
            && self
                .sensors
                .values()
                .all(|d| d.as_deref().is_some_and(valid_digest))
            && self.sensors.keys().eq(self.declarations.keys())
    }
}

/// SHA-256 spelling accepted at a freshness boundary.
pub fn valid_digest(value: &str) -> bool {
    const HEX_LENGTH: usize = 64;
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == HEX_LENGTH && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Canonical input identity for one sensor. Any uncertainty returns None.
pub fn sensor_digest(root: &Path, spec: &SensorSpec, config: Option<&[u8]>) -> Option<String> {
    let tools = snapshot_tools(root, &[spec]);
    sensor_digest_with_tools(root, spec, config, &tools)
}

type ToolSnapshot = BTreeMap<String, Option<(PathBuf, String)>>;

fn snapshot_tools(root: &Path, specs: &[&SensorSpec]) -> ToolSnapshot {
    let mut tools = BTreeMap::new();
    for program in specs.iter().filter_map(|s| s.argv.first()) {
        tools.entry(program.clone()).or_insert_with(|| {
            let path = executable(root, program)?;
            let digest = hash(&std::fs::read(&path).ok()?);
            Some((path, digest))
        });
    }
    tools
}

fn sensor_digest_with_tools(
    root: &Path,
    spec: &SensorSpec,
    config: Option<&[u8]>,
    tools: &ToolSnapshot,
) -> Option<String> {
    let sources = crate::sensor_inputs::effective_input_digest(root, spec, config)?;
    let mut files = BTreeMap::new();
    for path in POLICY_INPUTS {
        files.insert((*path).to_owned(), optional_file(root, path)?);
    }
    // Scripts are policy even when a narrowed inputs declaration omits them.
    for arg in &spec.argv {
        if root.join(arg).is_file() && !Path::new(arg).is_absolute() {
            files.insert(arg.clone(), optional_file(root, arg)?);
        }
    }
    let env: BTreeMap<String, String> = TOOL_ENV
        .iter()
        .copied()
        .chain(spec.environment_inputs.iter().map(String::as_str))
        .map(|key| (key.to_owned(), environment_identity(key)))
        .collect();
    let (tool, tool_hash) = tools.get(spec.argv.first()?)?.as_ref()?;
    let payload = serde_json::json!({
        "algorithm": ALGORITHM, "source": sources, "policy_inputs": files,
        "environment": env, "tool": tool.to_str()?, "tool_sha256": tool_hash,
        "branch": crate::changes::current_branch(root), "root": root.canonicalize().ok()?.to_str()?,
        "harness": crate::version::VersionInfo::current(),
    });
    Some(crate::fingerprint::hash_canonical(&payload))
}

fn optional_file(root: &Path, path: &str) -> Option<String> {
    let full = root.join(path);
    match std::fs::symlink_metadata(&full) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some("absent".into()),
        Ok(meta) if meta.is_file() => {
            if !full
                .canonicalize()
                .ok()?
                .starts_with(root.canonicalize().ok()?)
            {
                return None;
            }
            Some(hash(&std::fs::read(full).ok()?))
        }
        _ => None,
    }
}

fn environment_identity(key: &str) -> String {
    match std::env::var_os(key) {
        None => "absent".to_owned(),
        Some(value) => hash(value.as_encoded_bytes()),
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn executable(root: &Path, program: &str) -> Option<PathBuf> {
    let path = crate::shell::resolve_program(program);
    if path.is_absolute() {
        return path.canonicalize().ok();
    }
    if path.components().count() > 1 {
        return root.join(path).canonicalize().ok();
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        let candidate = dir.join(&path);
        if candidate.is_file() {
            return candidate.canonicalize().ok();
        }
        if cfg!(windows) {
            let candidate = candidate.with_extension("exe");
            if candidate.is_file() {
                return candidate.canonicalize().ok();
            }
        }
    }
    crate::shell::windows_batch_shim(program)?
        .canonicalize()
        .ok()
}

fn git_revision(root: &Path) -> Option<String> {
    let output = crate::changes::git_command(root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
