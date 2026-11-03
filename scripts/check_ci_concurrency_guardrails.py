#!/usr/bin/env python3
"""
check_ci_concurrency_guardrails.py — CI Concurrency & Timeout Guardrails Verification

Enforces the prevention pattern across workflows and concurrency tests:
1. The Windows workspace suite runs as a bounded shard matrix whose partitions
   cover every test exactly once, with compilation and execution timed apart.
2. The serialized do-harness-db Windows step stays separate from the shards.
3. Coverage job separation from normal Windows test jobs.
4. Concurrency test wave reduction under coverage instrumentation ((1,1) under coverage vs (2,3)).
5. CARGO_BUILD_JOBS <= 2 and CARGO_INCREMENTAL = 0 in CI workflows running Rust tasks.
6. All CI test/coverage steps or jobs carry bounded timeout-minutes.
7. Self-test mode (`--self-test`) validates negative mutation controls.
"""

import sys
import re
import argparse
import tempfile
from pathlib import Path

WINDOWS_TEST_STEP_MAX_MINUTES = 12
WINDOWS_SHARD_JOB = "windows-tests"
WINDOWS_DB_STEP_NAME = "Test do-harness-db on Windows"
WINDOWS_WORKSPACE_EXCLUDES = "do-harness-db"


def _job_body(content: str, job_name: str) -> str | None:
    """Return the text of a top-level job, or None when it is absent."""
    match = re.search(
        rf"\n  {re.escape(job_name)}:\n(.*?)(?=\n  [A-Za-z0-9_-]+:\n|\Z)",
        content,
        re.DOTALL,
    )
    return match.group(1) if match else None


def _steps(job_body: str) -> list[str]:
    return re.split(r"\n      - ", job_body)[1:]


def _step_name(step: str) -> str:
    match = re.search(r"^\s*name:\s*(.+)$", step, re.MULTILINE)
    return match.group(1).strip() if match else ""


def _step_timeout(step: str) -> int | None:
    match = re.search(r"^\s*timeout-minutes:\s*(\d+)\s*$", step, re.MULTILINE)
    return int(match.group(1)) if match else None


def _step_run(step: str) -> str:
    """Flatten a step's `run:` scalar, whether inline or a block."""
    match = re.search(r"^\s*run:\s*(.*)$", step, re.MULTILINE)
    if not match:
        return ""
    first = match.group(1).strip()
    if first and first not in ("|", ">", "|-", ">-", "|+", ">+"):
        return first
    lines = []
    for line in step[match.end():].splitlines():
        if not line.strip():
            continue
        if len(line) - len(line.lstrip()) <= 8:
            break
        lines.append(line.strip())
    return " ".join(lines)


def check_windows_shard_structure(verify_yml_path: Path) -> list[str]:
    errors = []
    content = verify_yml_path.read_text(encoding="utf-8")

    shard_body = _job_body(content, WINDOWS_SHARD_JOB)
    if shard_body is None:
        errors.append(
            f"verify.yml must define a `{WINDOWS_SHARD_JOB}` job for the sharded Windows workspace suite"
        )
        return errors

    if "runs-on: windows-latest" not in shard_body:
        errors.append(f"verify.yml `{WINDOWS_SHARD_JOB}` job must run on windows-latest")

    if re.search(r"^\s*continue-on-error:\s*true", shard_body, re.MULTILINE):
        errors.append(
            f"verify.yml `{WINDOWS_SHARD_JOB}` job must not set continue-on-error: every shard is required"
        )

    shard_match = re.search(r"^\s*shard:\s*\[([0-9,\s]+)\]", shard_body, re.MULTILINE)
    if not shard_match:
        errors.append(
            f"verify.yml `{WINDOWS_SHARD_JOB}` job must declare an explicit `shard: [1, 2, ...]` matrix"
        )
        shards: list[int] = []
    else:
        shards = [int(value) for value in shard_match.group(1).split(",") if value.strip()]
        if shards != list(range(1, len(shards) + 1)):
            errors.append(
                f"verify.yml `{WINDOWS_SHARD_JOB}` shard matrix must be contiguous from 1, got {shards}"
            )

    steps = _steps(shard_body)
    compile_steps = [step for step in steps if "compile" in _step_name(step).lower()]
    test_steps = [step for step in steps if "run workspace tests" in _step_name(step).lower()]

    if len(compile_steps) != 1:
        errors.append(
            f"verify.yml `{WINDOWS_SHARD_JOB}` must have exactly one named compile step, found {len(compile_steps)}"
        )
    if len(test_steps) != 1:
        errors.append(
            f"verify.yml `{WINDOWS_SHARD_JOB}` must have exactly one named test step, found {len(test_steps)}"
        )

    for label, step in (
        ("compile", compile_steps[0] if compile_steps else None),
        ("test", test_steps[0] if test_steps else None),
    ):
        if step is None:
            continue
        timeout = _step_timeout(step)
        if timeout is None:
            errors.append(f"verify.yml Windows {label} step is missing timeout-minutes")
        elif timeout > WINDOWS_TEST_STEP_MAX_MINUTES:
            errors.append(
                f"verify.yml Windows {label} step timeout-minutes ({timeout}) exceeds maximum allowed "
                f"{WINDOWS_TEST_STEP_MAX_MINUTES} minutes"
            )

    if compile_steps:
        run = _step_run(compile_steps[0])
        if "--no-run" not in run:
            errors.append("verify.yml Windows compile step must use `cargo nextest run --no-run`")
        if "--release" not in run:
            errors.append("verify.yml Windows compile step must build in release mode")
        if f"--exclude {WINDOWS_WORKSPACE_EXCLUDES}" not in run:
            errors.append(
                f"verify.yml Windows compile step must exclude {WINDOWS_WORKSPACE_EXCLUDES}"
            )

    if test_steps:
        step = test_steps[0]
        run = _step_run(step)
        if "matrix.shard" not in _step_name(step):
            errors.append("verify.yml Windows test step name must identify the shard")
        if "--release" not in run:
            errors.append("verify.yml Windows test step must run in release mode")
        if f"--exclude {WINDOWS_WORKSPACE_EXCLUDES}" not in run:
            errors.append(
                f"verify.yml Windows test step must exclude {WINDOWS_WORKSPACE_EXCLUDES}"
            )
        if "-p do-harness-db" in run:
            errors.append(
                "verify.yml Windows shard test step must not include do-harness-db; it has its own serialized step"
            )
        partition = re.search(
            r"--partition\s+count:\$\{\{\s*matrix\.shard\s*\}\}/(\d+)", run
        )
        if not partition:
            errors.append(
                "verify.yml Windows test step must partition with `--partition count:${{ matrix.shard }}/<shard-count>`"
            )
        elif shards and int(partition.group(1)) != len(shards):
            errors.append(
                f"verify.yml Windows test partition denominator ({partition.group(1)}) must equal "
                f"the shard matrix size ({len(shards)})"
            )
        if "--final-status-level" not in run:
            errors.append(
                "verify.yml Windows test step must surface slow tests with `--final-status-level`"
            )
        if "--no-fail-fast" not in run:
            errors.append(
                "verify.yml Windows test step must report every failure with `--no-fail-fast`"
            )

    return errors


def check_windows_job(verify_yml_path: Path) -> list[str]:
    errors = []
    content = verify_yml_path.read_text(encoding="utf-8")

    body = _job_body(content, "windows")
    if body is None:
        errors.append("verify.yml must define a `windows` job")
        return errors

    if "cargo llvm-cov run" in body or "cargo llvm-cov nextest" in body:
        errors.append("verify.yml windows job must not run memory-heavy coverage suite")

    steps = _steps(body)
    db_steps = [step for step in steps if WINDOWS_DB_STEP_NAME in _step_name(step)]
    if len(db_steps) != 1:
        errors.append(
            f"verify.yml windows job must keep exactly one serialized `{WINDOWS_DB_STEP_NAME}` step, "
            f"found {len(db_steps)}"
        )
    else:
        step = db_steps[0]
        if _step_timeout(step) is None:
            errors.append(f"verify.yml `{WINDOWS_DB_STEP_NAME}` step is missing timeout-minutes")
        run = _step_run(step)
        if "-p do-harness-db" not in run:
            errors.append(
                f"verify.yml `{WINDOWS_DB_STEP_NAME}` step must target -p do-harness-db"
            )
        if "--test-threads=1" not in run:
            errors.append(
                f"verify.yml `{WINDOWS_DB_STEP_NAME}` step must stay serialized with --test-threads=1"
            )

    for step in steps:
        if "cargo nextest run --workspace" in _step_run(step):
            errors.append(
                "verify.yml windows job must not run the unsharded workspace suite; it belongs to the "
                f"`{WINDOWS_SHARD_JOB}` shards"
            )

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
        errors.extend(check_windows_shard_structure(verify_yml))
        errors.extend(check_windows_job(verify_yml))

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

        verify_text = v_yml.read_text(encoding="utf-8")
        concurrency_text = c_rs.read_text(encoding="utf-8")

        (wf_dir / "verify.yml").write_text(verify_text, encoding="utf-8")
        (wf_dir / "scheduled.yml").write_text(s_yml.read_text(encoding="utf-8"), encoding="utf-8")
        (wf_dir / "release.yml").write_text(r_yml.read_text(encoding="utf-8"), encoding="utf-8")
        (tests_dir / "concurrency.rs").write_text(concurrency_text, encoding="utf-8")

        base_errs = run_guardrail_checks(tmp_path)
        assert not base_errs, f"Base temp copy failed: {base_errs}"

        def expect(mutated_verify: str, needle: str, label: str, mutated_concurrency: str | None = None):
            (wf_dir / "verify.yml").write_text(mutated_verify, encoding="utf-8")
            (tests_dir / "concurrency.rs").write_text(
                mutated_concurrency if mutated_concurrency is not None else concurrency_text,
                encoding="utf-8",
            )
            errs = run_guardrail_checks(tmp_path)
            assert any(needle in e for e in errs), f"{label} not caught: {errs}"

        # Mutation A: a Windows shard step timeout above the ceiling.
        expect(
            verify_text.replace("timeout-minutes: 12", "timeout-minutes: 30"),
            "exceeds maximum allowed 12",
            "Mutation A (timeout > 12)",
        )

        # Mutation B: remove the coverage wave reduction.
        expect(
            verify_text,
            "reduce waves to (1, 1)",
            "Mutation B (wave reduction)",
            mutated_concurrency=concurrency_text.replace("(1, 1)", "(2, 3)"),
        )

        # Mutation C: CARGO_BUILD_JOBS above the ceiling.
        expect(
            verify_text.replace('CARGO_BUILD_JOBS: "2"', 'CARGO_BUILD_JOBS: "8"'),
            "CARGO_BUILD_JOBS",
            "Mutation C (CARGO_BUILD_JOBS > 2)",
        )

        # Mutation D: coverage inside the Windows job.
        expect(
            verify_text.replace(
                "run: cargo test -p do-harness-db --release -- --test-threads=1",
                "run: cargo llvm-cov nextest run -p do-harness-db --release -- --test-threads=1",
            ),
            "must not run memory-heavy coverage",
            "Mutation D (windows coverage)",
        )

        # Mutation E: a shard dropped from the matrix while the partition still
        # divides by three, so the union no longer covers the suite.
        expect(
            verify_text.replace("shard: [1, 2, 3]", "shard: [1, 2]"),
            "must equal the shard matrix size",
            "Mutation E (incomplete shard matrix)",
        )

        # Mutation F: partition denominator disagrees with the shard count.
        expect(
            verify_text.replace("count:${{ matrix.shard }}/3", "count:${{ matrix.shard }}/4"),
            "partition denominator",
            "Mutation F (partition denominator)",
        )

        # Mutation G: the compile step loses its bounded timeout.
        expect(
            re.sub(
                r"        timeout-minutes: 12\n        run: cargo nextest run --workspace --release --exclude do-harness-db --no-run",
                "        run: cargo nextest run --workspace --release --exclude do-harness-db --no-run",
                verify_text,
            ),
            "Windows compile step is missing timeout-minutes",
            "Mutation G (compile timeout removed)",
        )

        # Mutation H: the serialized do-harness-db step is folded away.
        expect(
            re.sub(
                r"      - name: Test do-harness-db on Windows\n(?:.*\n)*?(?=      - name: |\Z)",
                "",
                verify_text,
            ),
            "must keep exactly one serialized",
            "Mutation H (db step removed)",
        )

        # Mutation I: the compile step stops being a compile-only step.
        expect(
            verify_text.replace(
                "run: cargo nextest run --workspace --release --exclude do-harness-db --no-run",
                "run: cargo nextest run --workspace --release --exclude do-harness-db",
            ),
            "must use `cargo nextest run --no-run`",
            "Mutation I (compile step runs tests)",
        )

        # Mutation J: the shard stops reporting every failure.
        expect(
            verify_text.replace(" --no-fail-fast", ""),
            "must report every failure",
            "Mutation J (no-fail-fast removed)",
        )

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
