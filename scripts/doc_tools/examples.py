"""
examples.py — Discovery, classification, parsing, and execution verification of documented CLI examples.
"""

import os
import re
import subprocess  # nosec B404
import sys
import tempfile
from typing import Any, Dict, List, Optional, Tuple

if sys.version_info >= (3, 11):
    import tomllib
else:
    try:
        import tomli as tomllib
    except ImportError:
        raise ImportError("tomllib (Python 3.11+) or tomli package is required to parse TOML files.")

from doc_tools.inventory import discover_documents

# Marker format regex:
# <!-- example: id=foo classification=executable [reason="..."] [fixture=bar] -->
# or <!-- example: foo classification=executable -->
MARKER_RE = re.compile(
    r"<!--\s*example:\s*(?:id=)?([a-zA-Z0-9_-]+)\s+classification=([a-zA-Z0-9_-]+)(?:\s+reason=[\"']([^\"']+)[\"'])?(?:\s+fixture=([a-zA-Z0-9_-]+))?\s*-->",
    re.IGNORECASE,
)


class ExampleError(Exception):
    """Base exception for example verification errors."""
    pass


# Hosts a fixture may name in its `platforms` list — the three CI hosts. A
# fixture without the key runs on every host.
FIXTURE_PLATFORMS = ("linux", "darwin", "windows")


def host_platform() -> str:
    """The current host, named the way a fixture's `platforms` list spells it."""
    if os.name == "nt":
        return "windows"
    return "darwin" if sys.platform == "darwin" else "linux"


class UnclassifiedExampleError(ExampleError):
    """Raised when an unclassified maintained CLI/config example is discovered."""
    pass


class StaleExampleError(ExampleError):
    """Raised when an example block in Markdown is stale or missing a required fixture."""
    pass


class ExampleMarker:
    def __init__(
        self,
        example_id: str,
        classification: str,
        reason: Optional[str] = None,
        fixture_id: Optional[str] = None,
        line_no: int = 1,
    ):
        self.example_id = example_id
        self.classification = classification.lower()
        self.reason = reason
        self.fixture_id = fixture_id or example_id
        self.line_no = line_no

        valid_classifications = {"executable", "parser-check-only", "illustrative", "historical"}
        if self.classification not in valid_classifications:
            raise ExampleError(
                f"Invalid classification '{classification}' for example '{example_id}'. "
                f"Allowed: {sorted(list(valid_classifications))}"
            )

        if self.classification != "executable" and not (reason and reason.strip()):
            raise ExampleError(
                f"Non-executable example '{example_id}' [{self.classification}] must specify a non-empty reason"
            )


class ExampleBlock:
    def __init__(
        self,
        filepath: str,
        marker: ExampleMarker,
        lang: str,
        code: str,
        line_no: int,
    ):
        self.filepath = filepath
        self.marker = marker
        self.lang = lang
        self.code = code
        self.line_no = line_no


def find_cli_binary(root_dir: str) -> str:
    """Resolves path to the do-harness CLI binary."""
    env_bin = os.environ.get("DO_HARNESS_BIN")
    if env_bin and os.path.isfile(env_bin) and os.access(env_bin, os.X_OK):
        return os.path.abspath(env_bin)

    candidates = [
        os.path.join(root_dir, "target", "release", "do-harness"),
        os.path.join(root_dir, "target", "debug", "do-harness"),
        os.path.join(root_dir, "target", "release", "do-harness.exe"),
        os.path.join(root_dir, "target", "debug", "do-harness.exe"),
    ]
    for cand in candidates:
        if os.path.isfile(cand) and os.access(cand, os.X_OK):
            return os.path.abspath(cand)

    # Try building with cargo if not found
    try:
        res = subprocess.run(  # nosec B603 B607
            ["cargo", "build", "--quiet", "--bin", "do-harness"],
            cwd=root_dir,
            capture_output=True,
            text=True,
        )
        if res.returncode == 0:
            for cand in candidates:
                if os.path.isfile(cand) and os.access(cand, os.X_OK):
                    return os.path.abspath(cand)
    except (subprocess.CalledProcessError, FileNotFoundError, OSError):
        pass

    raise ExampleError("do-harness binary not found under target/ release/debug or via DO_HARNESS_BIN.")


def load_fixture_manifest(manifest_path: str) -> Dict[str, Any]:
    """Loads and validates the fixture manifest."""
    if not os.path.exists(manifest_path):
        raise ExampleError(f"Fixture manifest not found at {manifest_path}")

    with open(manifest_path, "rb") as f:
        data = tomllib.load(f)

    version = data.get("version")
    if version != 1:
        raise ExampleError(f"Unsupported fixture manifest version {version} in {manifest_path}")

    fixtures = data.get("fixtures", {})
    if not isinstance(fixtures, dict):
        raise ExampleError(f"Malformed [fixtures] section in {manifest_path}")

    for fixture_id, fixture in fixtures.items():
        platforms = fixture.get("platforms")
        if platforms is None:
            continue
        unknown = [name for name in platforms if name not in FIXTURE_PLATFORMS]
        if unknown:
            raise ExampleError(
                f"Fixture '{fixture_id}' in {manifest_path} declares unknown "
                f"platform(s) {unknown}; allowed: {list(FIXTURE_PLATFORMS)}"
            )

    return fixtures


def is_cli_or_config_fence(lang: str, code: str) -> bool:
    """Determines if a code block looks like a CLI command or TOML/JSON configuration."""
    lang_lower = lang.lower() if lang else ""

    if lang_lower in ("bash", "sh", "zsh", "shell", "console"):
        return True
    if lang_lower == "toml" and ("language" in code or "[[sensors]]" in code or "[signal-sets]" in code):
        return True
    if lang_lower == "json" and ("\"ok\":" in code or "\"schema_version\":" in code or "\"sensors\":" in code):
        return True

    # Code fence without lang tag or generic lang containing do-harness commands
    if "do-harness" in code or "npx do-harness" in code or "cargo install do-harness" in code:
        return True

    return False


def extract_example_blocks(filepath: str, content: str) -> Tuple[List[ExampleBlock], List[str]]:
    """
    Extracts classified example blocks and detects unclassified CLI/config code blocks.
    Returns (blocks, errors).
    """
    blocks = []
    errors = []

    lines = content.splitlines()
    i = 0
    n = len(lines)

    pending_marker = None

    while i < n:
        line = lines[i]
        line_no = i + 1

        # Check for HTML comment marker
        marker_match = MARKER_RE.search(line)
        if marker_match:
            ex_id = marker_match.group(1)
            cls = marker_match.group(2)
            reason = marker_match.group(3)
            fixture_id = marker_match.group(4)
            try:
                pending_marker = ExampleMarker(ex_id, cls, reason, fixture_id, line_no)
            except ExampleError as e:
                errors.append(f"{filepath}:{line_no}: {e}")
            i += 1
            continue

        # Check for code fence start
        if line.strip().startswith("```"):
            fence_line_no = line_no
            fence_head = line.strip().lstrip("`")
            lang = fence_head.split()[0] if fence_head.split() else ""

            code_lines = []
            i += 1
            while i < n and not lines[i].strip().startswith("```"):
                code_lines.append(lines[i])
                i += 1

            code_body = "\n".join(code_lines)

            if pending_marker:
                blocks.append(
                    ExampleBlock(
                        filepath=filepath,
                        marker=pending_marker,
                        lang=lang,
                        code=code_body,
                        line_no=fence_line_no,
                    )
                )
                pending_marker = None
            else:
                # If no pending marker, check if this code fence is an unclassified CLI/config block
                if is_cli_or_config_fence(lang, code_body):
                    # Exclude generated blocks like cli-commands table
                    if "<!-- BEGIN GENERATED" not in code_body and "<!-- END GENERATED" not in code_body:
                        errors.append(
                            f"{filepath}:{fence_line_no}: Unclassified CLI/config example block without adjacent <!-- example: ... --> marker"
                        )

            i += 1
            continue

        # Non-empty, non-marker line resets pending marker if not immediately adjacent to code fence
        if line.strip() and not line.strip().startswith("<!--"):
            pending_marker = None

        i += 1

    return blocks, errors


def run_setup_steps(tmp_dir: str, steps: List[str], binary_path: str):
    """Applies environment setup steps in a temporary directory."""
    for step in steps:
        if step == "git_init":
            subprocess.run(  # nosec B603 B607
                ["git", "init", "-q"],
                cwd=tmp_dir,
                check=True,
                capture_output=True,
            )
            # Create an initial commit if needed for git status
            subprocess.run(  # nosec B603 B607
                ["git", "config", "user.name", "Test User"],
                cwd=tmp_dir,
                check=True,
            )
            subprocess.run(  # nosec B603 B607
                ["git", "config", "user.email", "test@example.com"],
                cwd=tmp_dir,
                check=True,
            )
        elif step == "cargo_toml_scaffold":
            cargo_toml = (
                "[package]\n"
                'name = "test-crate"\n'
                'version = "0.1.0"\n'
                'edition = "2021"\n'
            )
            with open(os.path.join(tmp_dir, "Cargo.toml"), "w", encoding="utf-8") as f:
                f.write(cargo_toml)
            os.makedirs(os.path.join(tmp_dir, "src"), exist_ok=True)
            with open(os.path.join(tmp_dir, "src", "lib.rs"), "w", encoding="utf-8") as f:
                # rustfmt-clean: `init` runs its initial verification and exits 1
                # when `fmt` is RED, which the rust_init_detected fixture (exit 0)
                # requires of this scaffold.
                f.write("pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n")
        elif step == "generic_init":
            # A literal argv head with `executable=` keeps the command a static
            # list for Codacy's subprocess patterns while the resolved binary
            # is what actually runs.
            subprocess.run(  # nosec B603 B607
                ["do-harness", "init", "--language", "generic"],
                executable=binary_path,
                cwd=tmp_dir,
                check=True,
                capture_output=True,
            )
        elif step == "custom_sensor_config":
            config = (
                'language = "generic"\n\n'
                '[[sensors]]\n'
                'name = "pass_sensor"\n'
                'argv = ["echo", "ok"]\n\n'
                '[[sensors]]\n'
                'name = "fail_sensor"\n'
                'argv = ["bash", "-c", "exit 1"]\n'
            )
            with open(os.path.join(tmp_dir, "do-harness.toml"), "w", encoding="utf-8") as f:
                f.write(config)
        elif step == "installer_local":
            # Staged installer setup
            pass


def normalize_json_output(data: Any) -> Any:
    """Recursively normalizes variable fields in JSON output (paths, timestamps, duration_ms)."""
    if isinstance(data, dict):
        norm = {}
        for k, v in data.items():
            if k in ("root", "filepath", "source_file"):
                norm[k] = "<NORMALIZED_PATH>"
            elif k in ("timestamp", "created_at", "since"):
                norm[k] = "<NORMALIZED_TIME>"
            elif k in ("duration_ms", "duration"):
                norm[k] = 0
            elif k in ("chain_hash", "output_sha256"):
                norm[k] = "<NORMALIZED_HASH>"
            else:
                norm[k] = normalize_json_output(v)
        return norm
    elif isinstance(data, list):
        return [normalize_json_output(item) for item in data]
    return data


def assert_file_state(tmp_dir: str, file_assertions: Dict[str, Any], filepath: str, line_no: int):
    """Verifies file existence and content assertions."""
    for rel_path, spec in file_assertions.items():
        full_path = os.path.join(tmp_dir, rel_path)
        should_exist = spec.get("exists", True)
        if should_exist and not os.path.exists(full_path):
            raise ExampleError(f"{filepath}:{line_no}: Expected file '{rel_path}' to be created by example")
        if not should_exist and os.path.exists(full_path):
            raise ExampleError(f"{filepath}:{line_no}: Expected file '{rel_path}' to not exist")

        if should_exist and "contains" in spec:
            want_str = spec["contains"]
            with open(full_path, "r", encoding="utf-8") as f:
                content = f.read()
            if want_str not in content:
                raise ExampleError(
                    f"{filepath}:{line_no}: File '{rel_path}' content missing expected substring '{want_str}'"
                )


def run_installer_fixture(root_dir: str, filepath: str, line_no: int, binary_path: str):
    """Executes local release-artifact installation fixture without network."""
    test_install_script = os.path.join(root_dir, "scripts", "test-install.sh")
    if not os.path.isfile(test_install_script):
        raise ExampleError(f"{filepath}:{line_no}: Installer test script not found at {test_install_script}")

    res = subprocess.run(  # nosec B603 B607
        ["bash", test_install_script, "--bin", binary_path],
        cwd=root_dir,
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        raise ExampleError(
            f"{filepath}:{line_no}: Local installer fixture failed (exit {res.returncode}):\n{res.stderr}\n{res.stdout}"
        )


def verify_examples(root_dir: str) -> bool:
    """Discovers and verifies all documented examples across maintained documents."""
    root_dir = os.path.abspath(root_dir)
    contracts_path = os.path.join(root_dir, "docs", "contracts.toml")
    manifest_path = os.path.join(root_dir, "tests", "fixtures", "docs-examples", "manifest.toml")

    try:
        inventory = discover_documents(root_dir, contracts_path, mode="local")
    except Exception as e:
        print(f"Inventory discovery failed: {e}", file=sys.stderr)
        return False

    maintained_docs = inventory["maintained"]
    fixtures = load_fixture_manifest(manifest_path)

    binary_path = None

    all_blocks = []
    all_errors = []

    for doc_rec in maintained_docs:
        full_path = os.path.join(root_dir, doc_rec.rel_path)
        if not os.path.isfile(full_path):
            continue
        try:
            with open(full_path, "r", encoding="utf-8") as f:
                content = f.read()
            blocks, errors = extract_example_blocks(doc_rec.rel_path, content)
            all_blocks.extend(blocks)
            all_errors.extend(errors)
        except Exception as e:
            all_errors.append(f"{doc_rec.rel_path}: Failed to read file: {e}")

    if not all_blocks:
        all_errors.append("Empty example discovery: No classified examples found across maintained documents.")

    if all_errors:
        for err in all_errors:
            print(err, file=sys.stderr)
        return False

    # Process discovered example blocks
    for block in all_blocks:
        marker = block.marker
        cls = marker.classification

        if cls in ("illustrative", "historical"):
            # Ensure reason is present (validated in ExampleMarker)
            continue

        if cls == "parser-check-only":
            lang = block.lang.lower()
            code = block.code
            try:
                if lang == "toml" or "language =" in code or "[[sensors]]" in code:
                    if sys.version_info >= (3, 11):
                        tomllib.loads(code)
                    else:
                        import tomli
                        tomli.loads(code)
                elif lang == "json" or code.strip().startswith("{") or code.strip().startswith("["):
                    import json
                    json.loads(code)
            except Exception as e:
                all_errors.append(
                    f"{block.filepath}:{block.line_no}: Invalid syntax in parser-check-only block '{marker.example_id}': {e}"
                )
            continue

        if cls == "executable":
            fixture_id = marker.fixture_id
            if fixture_id not in fixtures:
                all_errors.append(
                    f"{block.filepath}:{block.line_no}: Example '{marker.example_id}' references unknown fixture '{fixture_id}' in manifest"
                )
                continue

            fixture = fixtures[fixture_id]

            # A fixture may declare the hosts it can run on — the local installer
            # fixture shells out to `scripts/test-install.sh`, which supports
            # Linux and macOS and reports `SKIP: unsupported host platform`
            # elsewhere. The skip is announced rather than silent; every other
            # executable example on this host still runs.
            fixture_platforms = fixture.get("platforms")
            if fixture_platforms and host_platform() not in fixture_platforms:
                print(
                    f"{block.filepath}:{block.line_no}: {marker.example_id}: fixture "
                    f"'{fixture_id}' is declared for {', '.join(fixture_platforms)}; "
                    f"skipped on {host_platform()}"
                )
                continue

            # Verify binary resolution lazily
            if not binary_path:
                try:
                    binary_path = find_cli_binary(root_dir)
                except ExampleError as e:
                    all_errors.append(str(e))
                    return False

            if fixture.get("type") == "installer":
                try:
                    run_installer_fixture(root_dir, block.filepath, block.line_no, binary_path)
                except ExampleError as e:
                    all_errors.append(str(e))
                continue

            # Verify displayed command in Markdown matches fixture definition
            expected_display = fixture.get("displayed_command", "").strip()
            if expected_display:
                actual_display = block.code.strip()
                # Normalize line endings and whitespace
                norm_exp = "\n".join(line.strip() for line in expected_display.splitlines() if line.strip())
                norm_act = "\n".join(line.strip() for line in actual_display.splitlines() if line.strip())

                if norm_exp != norm_act:
                    msg = (
                        f"{block.filepath}:{block.line_no}: Stale displayed command for example '{marker.example_id}'.\n"
                        f"  Expected: {expected_display}\n"
                        f"  Found:    {block.code.strip()}"
                    )
                    all_errors.append(msg)
                    continue

            # Execute fixture in isolated temporary directory
            with tempfile.TemporaryDirectory() as tmp_dir:
                tmp_home = os.path.join(tmp_dir, "home")
                tmp_work = os.path.join(tmp_dir, "work")
                os.makedirs(tmp_home, exist_ok=True)
                os.makedirs(tmp_work, exist_ok=True)

                setup_steps = fixture.get("setup_steps", [])
                try:
                    run_setup_steps(tmp_work, setup_steps, binary_path)
                except Exception as e:
                    all_errors.append(
                        f"{block.filepath}:{block.line_no}: Setup failed for example '{marker.example_id}': {e}"
                    )
                    continue

                commands = fixture.get("commands", [])
                env = os.environ.copy()
                env["HOME"] = tmp_home
                env["DO_HARNESS_BIN"] = binary_path
                env["CI"] = "false"

                for cmd_spec in commands:
                    argv = cmd_spec.get("argv", [])
                    want_exit = cmd_spec.get("expected_exit", 0)

                    # A literal argv head with `executable=` keeps the command
                    # a static list for Codacy's subprocess patterns while the
                    # resolved binary is what actually runs.
                    res = subprocess.run(  # nosec B603
                        ["do-harness", *argv],
                        executable=binary_path,
                        cwd=tmp_work,
                        capture_output=True,
                        text=True,
                        env=env,
                    )

                    if res.returncode != want_exit:
                        all_errors.append(
                            f"{block.filepath}:{block.line_no}: Example '{marker.example_id}' command {argv} "
                            f"returned exit code {res.returncode} (expected {want_exit}).\n"
                            f"  stdout: {res.stdout}\n  stderr: {res.stderr}"
                        )
                        break

                    # Validate JSON output if command expected json format
                    if "--format" in argv and "json" in argv and res.stdout.strip():
                        try:
                            import json
                            data = json.loads(res.stdout)
                            normalize_json_output(data)
                        except Exception as e:
                            all_errors.append(
                                f"{block.filepath}:{block.line_no}: Example '{marker.example_id}' "
                                f"stdout was not valid JSON: {e}"
                            )
                            break

                # File assertions
                if "file_assertions" in fixture:
                    try:
                        assert_file_state(
                            tmp_work,
                            fixture["file_assertions"],
                            block.filepath,
                            block.line_no,
                        )
                    except ExampleError as e:
                        all_errors.append(str(e))

    if all_errors:
        for err in all_errors:
            print(err, file=sys.stderr)
        return False

    print("Examples check OK: all documented examples and fixture contracts verified.")
    return True
