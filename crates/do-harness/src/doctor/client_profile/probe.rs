//! Explicit, bounded adapter execution for `doctor --client-profile --client-probe`.
//!
//! Nothing here runs unless the operator asks for it. The adapter is trusted
//! local test code, not a sandbox: it can fabricate observations, so its digest
//! is pinned and its output is bounded and validated.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::exit::UsageError;

use super::Reason;
use super::expected_route;
use super::observation::{
    AdapterIdentity, ClaimObservation, GOVERNANCE_MODES, GovernanceObservation,
    OBSERVATION_SCHEMA_VERSION, OBSERVED_REASONS, OBSERVED_STATUSES, Observations,
};
use super::profile::{Adapter, ClaimRequirement, ClientPin};

/// Protocol version spoken with a trusted adapter.
const PROBE_PROTOCOL_VERSION: u32 = 1;

/// Bound on adapter execution; a hung adapter must not pin `doctor`.
const PROBE_TIMEOUT_SECONDS: u64 = 5;

/// Bound on either adapter stream, in bytes.
const MAX_PROBE_STREAM_BYTES: u64 = 64 * 1024;

/// Freshness window applied to probed observations, in seconds.
const PROBE_MAX_AGE_SECONDS: u64 = 300;

/// Result of an explicitly requested probe.
pub(crate) enum ProbeOutcome {
    Observed(Box<Observations>),
    Failed(Reason),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeResponse {
    protocol_version: u32,
    #[serde(default)]
    governance: Option<ProbeGovernance>,
    observations: BTreeMap<String, ProbeObservation>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeGovernance {
    mode: String,
    route: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeObservation {
    status: String,
    reason: String,
}

/// Runs the pinned adapter once and turns its answer into an observation.
///
/// # Errors
///
/// A missing or digest-mismatched adapter is an operator problem (exit 2): the
/// pin cannot be honoured, so no claim was tested.
pub(crate) async fn run(
    adapter: &Adapter,
    client: &ClientPin,
    configuration: &str,
    claims: &BTreeMap<String, ClaimRequirement>,
    now: u64,
) -> Result<ProbeOutcome> {
    let program = adapter
        .argv
        .first()
        .context("adapter.argv is empty")
        .map_err(UsageError::wrap)?;
    let executable = resolve_executable(program).map_err(UsageError::wrap)?;
    let digest = file_sha256(&executable).map_err(UsageError::wrap)?;
    if digest != adapter.sha256 {
        return Err(UsageError::msg(format!(
            "adapter digest mismatch: profile pins {}, {} hashes to {digest}",
            adapter.sha256,
            executable.display()
        ))
        .into());
    }

    let marker = format!("synthetic_{}", hex::encode(random_bytes()));
    let request = serde_json::json!({
        "protocol_version": PROBE_PROTOCOL_VERSION,
        "client": { "id": client.id, "version": client.version },
        "configuration": configuration,
        "adapter_sha256": adapter.sha256,
        "synthetic_marker": marker,
        "claims": claims
            .iter()
            .map(|(id, requirement)| serde_json::json!({
                "id": id,
                "route": expected_route(id, requirement),
            }))
            .collect::<Vec<_>>(),
    });

    let workspace = tempfile::tempdir().context("cannot create a disposable probe workspace")?;
    let request_path = workspace.path().join("request.json");
    write_request(&request_path, &request)?;
    let stdout_path = workspace.path().join("stdout");
    let stderr_path = workspace.path().join("stderr");

    let stdin = std::fs::File::open(&request_path).context("cannot reopen the probe request")?;
    let stdout = std::fs::File::create(&stdout_path).context("cannot create probe stdout")?;
    let stderr = std::fs::File::create(&stderr_path).context("cannot create probe stderr")?;

    let mut command = tokio::process::Command::new(&executable);
    command
        .args(&adapter.argv[1..])
        .current_dir(workspace.path())
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .kill_on_drop(true);

    let mut child = command.spawn().map_err(UsageError::wrap)?;
    let waited =
        tokio::time::timeout(Duration::from_secs(PROBE_TIMEOUT_SECONDS), child.wait()).await;
    match waited {
        Err(_) => {
            let _ = child.kill().await;
            return Ok(ProbeOutcome::Failed(Reason::AdapterTimeout));
        }
        Ok(Err(err)) => return Err(UsageError::wrap(anyhow::Error::new(err)).into()),
        Ok(Ok(status)) if !status.success() => {
            return Ok(ProbeOutcome::Failed(Reason::AdapterFailed));
        }
        Ok(Ok(_)) => {}
    }

    if stream_len(&stdout_path) > MAX_PROBE_STREAM_BYTES
        || stream_len(&stderr_path) > MAX_PROBE_STREAM_BYTES
    {
        return Ok(ProbeOutcome::Failed(Reason::OversizedObservation));
    }
    let observed = std::fs::read(&stdout_path).unwrap_or_default();
    let diagnostics = std::fs::read(&stderr_path).unwrap_or_default();
    // A marker on either channel means the adapter echoed synthetic private
    // data; that is a dishonest observation, not a pass.
    if contains(&observed, &marker) || contains(&diagnostics, &marker) {
        return Ok(ProbeOutcome::Failed(Reason::MalformedObservation));
    }
    let Ok(response) = serde_json::from_slice::<ProbeResponse>(&observed) else {
        return Ok(ProbeOutcome::Failed(Reason::MalformedObservation));
    };
    match assemble(response, adapter, client, configuration, claims, now) {
        Ok(observations) => Ok(ProbeOutcome::Observed(Box::new(observations))),
        Err(reason) => Ok(ProbeOutcome::Failed(reason)),
    }
}

fn assemble(
    response: ProbeResponse,
    adapter: &Adapter,
    client: &ClientPin,
    configuration: &str,
    claims: &BTreeMap<String, ClaimRequirement>,
    now: u64,
) -> std::result::Result<Observations, Reason> {
    if response.protocol_version != PROBE_PROTOCOL_VERSION {
        return Err(Reason::MalformedObservation);
    }
    let mut observed = BTreeMap::new();
    for (id, requirement) in claims {
        let Some(route) = expected_route(id, requirement) else {
            continue;
        };
        let (status, reason) = match response.observations.get(id) {
            Some(claim)
                if OBSERVED_STATUSES.contains(&claim.status.as_str())
                    && OBSERVED_REASONS.contains(&claim.reason.as_str()) =>
            {
                (claim.status.clone(), claim.reason.clone())
            }
            Some(_) => return Err(Reason::MalformedObservation),
            None => ("unknown".to_owned(), "not-observed".to_owned()),
        };
        observed.insert(
            id.clone(),
            ClaimObservation {
                route: route.to_owned(),
                status,
                reason,
            },
        );
    }
    let governance = match response.governance {
        Some(governance) if GOVERNANCE_MODES.contains(&governance.mode.as_str()) => {
            Some(GovernanceObservation {
                mode: governance.mode,
                route: governance.route,
            })
        }
        Some(_) => return Err(Reason::MalformedObservation),
        None => None,
    };
    Ok(Observations {
        schema_version: OBSERVATION_SCHEMA_VERSION,
        client: client.clone(),
        adapter: AdapterIdentity {
            sha256: adapter.sha256.clone(),
        },
        configuration: configuration.to_owned(),
        generated_at_unix_seconds: now,
        max_age_seconds: PROBE_MAX_AGE_SECONDS,
        governance,
        claims: observed,
    })
}

fn write_request(path: &Path, request: &serde_json::Value) -> Result<()> {
    let mut file = std::fs::File::create(path).context("cannot create the probe request")?;
    file.write_all(request.to_string().as_bytes())
        .context("cannot write the probe request")
}

fn stream_len(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |meta| meta.len())
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// A synthetic marker only has to be unguessable within one run; `RandomState`
/// is seeded from OS entropy, so no crypto dependency is needed.
fn random_bytes() -> [u8; 16] {
    use std::hash::{BuildHasher, Hasher};

    let mut bytes = [0_u8; 16];
    for chunk in bytes.chunks_mut(8) {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        chunk.copy_from_slice(&hasher.finish().to_le_bytes()[..chunk.len()]);
    }
    bytes
}

fn resolve_executable(program: &str) -> Result<PathBuf> {
    let candidate = Path::new(program);
    if candidate.components().count() > 1 {
        return Ok(candidate.to_path_buf());
    }
    let path_var = std::env::var_os("PATH").context("PATH is unset")?;
    for dir in std::env::split_paths(&path_var) {
        let full = dir.join(program);
        if full.is_file() {
            return Ok(full);
        }
        #[cfg(windows)]
        {
            let with_extension = full.with_extension("exe");
            if with_extension.is_file() {
                return Ok(with_extension);
            }
        }
    }
    anyhow::bail!("adapter executable '{program}' not found on PATH")
}

fn file_sha256(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}
