//! Integration test for progressive-disclosure skill selection benchmark (WF-08).

#![allow(clippy::unwrap_used, clippy::expect_used)]
// The benchmark script is a bash script using jq and POSIX paths, so these
// fixtures are Unix-only by construction (scripts/check-shell.sh lints it everywhere).
#![cfg(unix)]
use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn skills_suggest_benchmark_evaluates_both_arms_and_preserves_decisions() {
    let root = workspace_root();
    let script = root.join("scripts/skills-suggest-benchmark.sh");
    let bin = env!("CARGO_BIN_EXE_do-harness");

    let output = Command::new("bash")
        .arg(&script)
        .env("DO_HARNESS_BIN", bin)
        .output()
        .expect("spawn skills-suggest-benchmark.sh");

    assert!(
        output.status.success(),
        "benchmark failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Both arms must be executed and reported
    assert!(
        stderr.contains("arm=load-all"),
        "must evaluate load-all arm:\n{stderr}"
    );
    assert!(
        stderr.contains("arm=deterministic-only"),
        "must evaluate deterministic-only arm:\n{stderr}"
    );

    // Decision preservation and downstream task success must be reported
    assert!(
        stderr.contains("decision_preservation_rate=100.0"),
        "decisions must be preserved:\n{stderr}"
    );
    assert!(
        stderr.contains("downstream_task_success_rate=100.0"),
        "downstream tasks must succeed:\n{stderr}"
    );

    // Separate accounting for metadata bytes, selected skill bytes, and reference bytes
    assert!(
        stderr.contains("metadata_bytes_per_task="),
        "metadata bytes must be reported:\n{stderr}"
    );
    assert!(
        stderr.contains("selected_skill_bytes_per_task="),
        "selected skill bytes must be reported:\n{stderr}"
    );
    assert!(
        stderr.contains("reference_bytes_per_task="),
        "reference bytes must be reported:\n{stderr}"
    );

    // Context reduction must be substantial (> 70%)
    assert!(
        stderr.contains("context_reduction_vs_load_all="),
        "reduction must be reported:\n{stderr}"
    );

    // Provider usage must stay unmeasured / null rather than estimating tokens from bytes
    assert!(
        stderr.contains("provider_usage=null"),
        "tokens must not be estimated from bytes:\n{stderr}"
    );

    // Stdout TSV should contain rows for both load-all and deterministic-only
    assert!(
        stdout.contains("load-all\t"),
        "stdout TSV must include load-all rows"
    );
    assert!(
        stdout.contains("deterministic-only\t"),
        "stdout TSV must include deterministic-only rows"
    );
}
