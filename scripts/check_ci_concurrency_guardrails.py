#!/usr/bin/env python3
"""
check_ci_concurrency_guardrails.py — CI Concurrency & Timeout Guardrails Verification

Enforces the prevention pattern across workflows and concurrency tests:
1. Windows CI test step timeout <= 12 minutes.
2. Coverage job separation from normal Windows test jobs.
3. Concurrency test wave reduction under coverage instrumentation ((1,1) under coverage vs (2,3)).
4. CARGO_BUILD_JOBS <= 2 and CARGO_INCREMENTAL = 0 in CI workflows running Rust tasks.
5. All CI test/coverage steps or jobs carry bounded timeout-minutes.
6. Self-test mode (`--self-test`) validates negative mutation controls.
"""

import sys
import re
import argparse
import tempfile
from pathlib import Path

def check_windows_test_timeout(verify_yml_path: Path) -> list[str]:
    errors = []
    content = verify_yml_path.read_text(encoding="utf-8")

    if "runs-on: windows-latest" not in content and "windows:" not in content:
        errors.append("verify.yml must contain a windows job running on windows-latest")
        return errors

    match = re.search(r"-\s+name:\s*Test workspace on Windows.*?\n\s+timeout-minutes:\s*(\d+)", content, re.DOTALL)
    if not match:
        errors.append("verify.yml Windows test step 'Test workspace on Windows' missing 'timeout-minutes'")
    else:
        timeout = int(match.group(1))
        if timeout > 12:
            errors.append(f"verify.yml Windows test step timeout-minutes ({timeout}) exceeds maximum allowed 12 minutes")

    return errors

def check_concurrency_test_wave_reduction(concurrency_rs_path: Path) -> list[str]:
    errors = []
    content = concurrency_rs_path.read_text(encoding="utf-8")

    if "LLVM_PROFILE_FILE" not in content and "CARGO_LLVM_COV" not in content:
        errors.append("concurrency.rs must check for LLVM_PROFILE_FILE or CARGO_LLVM_COV environment variables")

    if "(1, 1)" not in content or "(2, 3)" not in content:
        errors.append("concurrency.rs must reduce waves to (1, 1) under coverage and (2, 3) otherwise")

    return errors

def check_ci_env_bounds(workflow_paths: list[Path]) -> list[str]:
    errors = []
    for path in workflow_paths:
        if not path.exists():
            continue
        content = path.read_text(encoding="utf-8")
        if "cargo" not in content.lower():
            continue

        m_jobs = re.findall(r"CARGO_BUILD_JOBS:\s*[\"']?(\d+)[\"']?", content)
        if not m_jobs:
            errors.append(f"{path.name} missing or invalid CARGO_BUILD_JOBS setting")
        else:
            for val_str in m_jobs:
                val = int(val_str)
                if val > 2:
                    errors.append(f"{path.name} CARGO_BUILD_JOBS={val} exceeds maximum allowed 2")

        m_inc = re.findall(r"CARGO_INCREMENTAL:\s*[\"']?(\d+)[\"']?", content)
        if not m_inc:
            errors.append(f"{path.name} missing or invalid CARGO_INCREMENTAL setting")
        else:
            for val_str in m_inc:
                val = int(val_str)
                if val != 0:
                    errors.append(f"{path.name} CARGO_INCREMENTAL={val} must be 0")

    return errors

def check_test_coverage_timeouts(workflow_paths: list[Path]) -> list[str]:
    errors = []
    for path in workflow_paths:
        if not path.exists():
            continue
        content = path.read_text(encoding="utf-8")
        if "cargo nextest" not in content and "cargo test" not in content and "cargo llvm-cov" not in content:
            continue

        if "timeout-minutes:" not in content:
            errors.append(f"{path.name} runs cargo tests/coverage but has no timeout-minutes bounded guardrails")

    return errors

def check_coverage_separation(verify_yml_path: Path) -> list[str]:
    errors = []
    content = verify_yml_path.read_text(encoding="utf-8")

    windows_job_match = re.search(r"\n  windows:\n(.*?)(?=\n  \w+:|\Z)", content, re.DOTALL)
    if windows_job_match:
        windows_body = windows_job_match.group(1)
        if "cargo llvm-cov run" in windows_body or "cargo llvm-cov nextest" in windows_body:
            errors.append("verify.yml windows job must not run memory-heavy coverage suite")

    return errors

def run_guardrail_checks(root_dir: Path) -> list[str]:
    errors = []
    verify_yml = root_dir / ".github" / "workflows" / "verify.yml"
    concurrency_rs = root_dir / "crates" / "do-harness" / "tests" / "concurrency.rs"

    workflow_paths = [
        root_dir / ".github" / "workflows" / "verify.yml",
        root_dir / ".github" / "workflows" / "scheduled.yml",
        root_dir / ".github" / "workflows" / "release.yml",
    ]

    if not verify_yml.exists():
        errors.append(f"Missing {verify_yml}")
    else:
        errors.extend(check_windows_test_timeout(verify_yml))
        errors.extend(check_coverage_separation(verify_yml))

    if not concurrency_rs.exists():
        errors.append(f"Missing {concurrency_rs}")
    else:
        errors.extend(check_concurrency_test_wave_reduction(concurrency_rs))

    errors.extend(check_ci_env_bounds(workflow_paths))
    errors.extend(check_test_coverage_timeouts(workflow_paths))

    return errors

def run_self_tests(root_dir: Path):
    print("Running check_ci_concurrency_guardrails self-tests...")

    real_errors = run_guardrail_checks(root_dir)
    assert not real_errors, f"Real repository failed guardrails: {real_errors}"

    with tempfile.TemporaryDirectory() as tmpdir:
        tmp_path = Path(tmpdir)
        wf_dir = tmp_path / ".github" / "workflows"
        wf_dir.mkdir(parents=True)
        tests_dir = tmp_path / "crates" / "do-harness" / "tests"
        tests_dir.mkdir(parents=True)

        v_yml = root_dir / ".github" / "workflows" / "verify.yml"
        s_yml = root_dir / ".github" / "workflows" / "scheduled.yml"
        r_yml = root_dir / ".github" / "workflows" / "release.yml"
        c_rs = root_dir / "crates" / "do-harness" / "tests" / "concurrency.rs"

        (wf_dir / "verify.yml").write_text(v_yml.read_text(encoding="utf-8"))
        (wf_dir / "scheduled.yml").write_text(s_yml.read_text(encoding="utf-8"))
        (wf_dir / "release.yml").write_text(r_yml.read_text(encoding="utf-8"))
        (tests_dir / "concurrency.rs").write_text(c_rs.read_text(encoding="utf-8"))

        base_errs = run_guardrail_checks(tmp_path)
        assert not base_errs, f"Base temp copy failed: {base_errs}"

        # Mutation A: Timeout > 12
        mut_v = v_yml.read_text(encoding="utf-8").replace("timeout-minutes: 12", "timeout-minutes: 30")
        (wf_dir / "verify.yml").write_text(mut_v)
        mut_errs = run_guardrail_checks(tmp_path)
        assert any("exceeds maximum allowed 12" in e for e in mut_errs), f"Mutation A (timeout > 12) not caught: {mut_errs}"
        (wf_dir / "verify.yml").write_text(v_yml.read_text(encoding="utf-8"))

        # Mutation B: Remove wave reduction
        mut_c = c_rs.read_text(encoding="utf-8").replace("(1, 1)", "(2, 3)")
        (tests_dir / "concurrency.rs").write_text(mut_c)
        mut_errs_b = run_guardrail_checks(tmp_path)
        assert any("reduce waves to (1, 1)" in e for e in mut_errs_b), f"Mutation B (wave reduction) not caught: {mut_errs_b}"
        (tests_dir / "concurrency.rs").write_text(c_rs.read_text(encoding="utf-8"))

        # Mutation C: CARGO_BUILD_JOBS > 2
        mut_env = v_yml.read_text(encoding="utf-8").replace('CARGO_BUILD_JOBS: "2"', 'CARGO_BUILD_JOBS: "8"')
        (wf_dir / "verify.yml").write_text(mut_env)
        mut_errs_c = run_guardrail_checks(tmp_path)
        assert any("CARGO_BUILD_JOBS=8" in e or "CARGO_BUILD_JOBS" in e for e in mut_errs_c), f"Mutation C (CARGO_BUILD_JOBS > 2) not caught: {mut_errs_c}"
        (wf_dir / "verify.yml").write_text(v_yml.read_text(encoding="utf-8"))

        # Mutation D: Coverage in Windows job
        windows_coverage = v_yml.read_text(encoding="utf-8").replace(
            "run: cargo nextest run --workspace --release --exclude do-harness-db",
            "run: cargo llvm-cov nextest run --workspace --release --exclude do-harness-db"
        )
        (wf_dir / "verify.yml").write_text(windows_coverage)
        mut_errs_d = run_guardrail_checks(tmp_path)
        assert any("must not run memory-heavy coverage" in e for e in mut_errs_d), f"Mutation D (windows coverage) not caught: {mut_errs_d}"
        (wf_dir / "verify.yml").write_text(v_yml.read_text(encoding="utf-8"))

    print("Self-tests passed successfully!")

def main():
    parser = argparse.ArgumentParser(description="Check CI concurrency guardrails.")
    parser.add_argument("--root", type=Path, default=Path("."), help="Root repository directory")
    parser.add_argument("--self-test", action="store_true", help="Run self-tests and mutation controls")
    args = parser.parse_args()

    root_dir = args.root.resolve()

    if args.self_test:
        run_self_tests(root_dir)
        sys.exit(0)

    errors = run_guardrail_checks(root_dir)
    if errors:
        print("FAIL: CI concurrency guardrails violated:", file=sys.stderr)
        for err in errors:
            print(f"  - {err}", file=sys.stderr)
        sys.exit(1)

    print("OK: CI concurrency guardrails intact.")
    sys.exit(0)

if __name__ == "__main__":
    main()
