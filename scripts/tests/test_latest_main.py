#!/usr/bin/env python3
"""Hermetic behavioral regression tests for latest-main helper and workflow preflights."""

import os
import pathlib
import stat
import subprocess
import tempfile
import unittest

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
HELPER_SCRIPT = REPO_ROOT / ".agents" / "skills" / "harness" / "scripts" / "latest-main.sh"
ISSUE_PREFLIGHT = REPO_ROOT / ".agents" / "skills" / "issue-to-pr" / "scripts" / "preflight.sh"
PR_PREFLIGHT = REPO_ROOT / ".agents" / "skills" / "pr-triage" / "scripts" / "preflight.sh"
START_ITEM = REPO_ROOT / ".agents" / "skills" / "issue-to-pr" / "scripts" / "start-item.sh"

UNSET_GIT_VARS = [
    "GIT_DIR", "GIT_WORK_TREE", "GIT_IMPLICIT_WORK_TREE", "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES", "GIT_DISCOVERY_ACROSS_FILESYSTEM", "GIT_NAMESPACE",
    "GIT_PREFIX", "GIT_COMMON_DIR", "GIT_SHALLOW_FILE", "GIT_GRAFT_FILE",
    "GIT_REPLACE_REF_BASE", "GIT_NO_REPLACE_OBJECTS",
]


class BaseGitTestCase(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.base = pathlib.Path(self.temp_dir.name)

        self.git_env = os.environ.copy()
        for var in UNSET_GIT_VARS:
            self.git_env.pop(var, None)
        self.git_env.update({
            "GIT_AUTHOR_NAME": "Test User",
            "GIT_AUTHOR_EMAIL": "test@example.invalid",
            "GIT_COMMITTER_NAME": "Test User",
            "GIT_COMMITTER_EMAIL": "test@example.invalid",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": "/dev/null",
            "HOME": str(self.base),
        })

        self.origin = self.base / "origin.git"
        subprocess.run(["git", "init", "--bare", "-b", "main", str(self.origin)], env=self.git_env, check=True)
        self.seeder = self.base / "seeder"
        subprocess.run(["git", "clone", str(self.origin), str(self.seeder)], env=self.git_env, check=True)

        (self.seeder / "README.md").write_text("commit A\n", encoding="utf-8")
        self.git(self.seeder, "add", ".")
        self.git(self.seeder, "commit", "-m", "init A")
        self.git(self.seeder, "branch", "-M", "main")
        self.git(self.seeder, "push", "origin", "main")
        self.commit_a = self.git(self.seeder, "rev-parse", "HEAD").stdout.strip()
        self.git(self.origin, "symbolic-ref", "HEAD", "refs/heads/main")

        self.client = self.base / "client"
        subprocess.run(["git", "clone", str(self.origin), str(self.client)], env=self.git_env, check=True)

    def git(self, cwd, *args, check=True):
        return subprocess.run(
            ["git", "-C", str(cwd)] + list(args),
            env=self.git_env, check=check, capture_output=True, text=True
        )

    def advance_origin(self, msg="commit B"):
        (self.seeder / "README.md").write_text(f"{msg}\n", encoding="utf-8")
        self.git(self.seeder, "add", ".")
        self.git(self.seeder, "commit", "-m", msg)
        self.git(self.seeder, "push", "origin", "main")
        return self.git(self.seeder, "rev-parse", "HEAD").stdout.strip()

    def run_helper(self, args=None, cwd=None, env=None):
        cmd = ["bash", str(HELPER_SCRIPT)] + (args or [])
        return subprocess.run(
            cmd, cwd=cwd or str(self.client),
            env=(env if env is not None else self.git_env),
            capture_output=True, text=True
        )


class TestLatestMain(BaseGitTestCase):
    def test_remote_advancement(self):
        commit_b = self.advance_origin("commit B")
        res = self.run_helper()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(res.stdout.strip(), commit_b)
        self.assertIn(f"latest-main: fetched: {commit_b}", res.stderr)
        self.assertEqual(self.git(self.client, "rev-parse", "refs/remotes/origin/main").stdout.strip(), commit_b)
        self.assertEqual(self.git(self.client, "rev-parse", "HEAD").stdout.strip(), self.commit_a)

    def test_read_only(self):
        commit_b = self.advance_origin("commit B")

        # Snapshot FETCH_HEAD, refs, index, HEAD, and worktree before
        fetch_head = self.client / ".git" / "FETCH_HEAD"
        fetch_head_before = fetch_head.read_bytes() if fetch_head.exists() else None
        refs_before = self.git(self.client, "show-ref").stdout
        idx_before = self.git(self.client, "ls-files", "-s").stdout
        head_before = self.git(self.client, "rev-parse", "HEAD").stdout
        status_before = self.git(self.client, "status", "--porcelain").stdout

        res = self.run_helper(["--read-only"])
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(res.stdout.strip(), commit_b)
        self.assertIn(f"latest-main: advertised only; not fetched: {commit_b}", res.stderr)

        # origin/main remains at commit A, object B is absent, state unchanged
        self.assertEqual(self.git(self.client, "rev-parse", "refs/remotes/origin/main").stdout.strip(), self.commit_a)
        self.assertNotEqual(self.git(self.client, "cat-file", "-e", commit_b, check=False).returncode, 0)
        fetch_head_after = fetch_head.read_bytes() if fetch_head.exists() else None
        self.assertEqual(fetch_head_after, fetch_head_before)
        self.assertEqual(self.git(self.client, "show-ref").stdout, refs_before)
        self.assertEqual(self.git(self.client, "ls-files", "-s").stdout, idx_before)
        self.assertEqual(self.git(self.client, "rev-parse", "HEAD").stdout, head_before)
        self.assertEqual(self.git(self.client, "status", "--porcelain").stdout, status_before)

    def test_dirty_feature_branch(self):
        self.git(self.client, "checkout", "-b", "feat-1")
        (self.client / "staged.txt").write_text("staged content\n", encoding="utf-8")
        self.git(self.client, "add", "staged.txt")
        (self.client / "README.md").write_text("unstaged modification\n", encoding="utf-8")
        (self.client / "untracked.txt").write_text("untracked content\n", encoding="utf-8")

        commit_b = self.advance_origin("commit B")
        res = self.run_helper()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(res.stdout.strip(), commit_b)

        self.assertEqual((self.client / "staged.txt").read_text(encoding="utf-8"), "staged content\n")
        self.assertEqual((self.client / "README.md").read_text(encoding="utf-8"), "unstaged modification\n")
        self.assertEqual((self.client / "untracked.txt").read_text(encoding="utf-8"), "untracked content\n")
        self.assertEqual(self.git(self.client, "branch", "--show-current").stdout.strip(), "feat-1")

    def test_correct_target_with_git_env(self):
        commit_b = self.advance_origin("commit B")
        polluted_env = self.git_env.copy()
        polluted_env["GIT_DIR"] = str(self.seeder / ".git")
        polluted_env["GIT_COMMON_DIR"] = str(self.seeder / ".git")

        res = self.run_helper(["--root", str(self.client)], env=polluted_env)
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(res.stdout.strip(), commit_b)
        self.assertEqual(self.git(self.client, "rev-parse", "refs/remotes/origin/main").stdout.strip(), commit_b)

    def test_failure_closed(self):
        empty_dir = self.base / "empty"
        empty_dir.mkdir()
        self.assertEqual(self.run_helper(["--root", str(empty_dir)]).returncode, 2)

        no_remote = self.base / "no-remote"
        subprocess.run(["git", "init", str(no_remote)], env=self.git_env, check=True)
        self.assertEqual(self.run_helper(["--root", str(no_remote)]).returncode, 2)

        self.assertEqual(self.run_helper(["--invalid"]).returncode, 2)
        self.assertEqual(self.run_helper(["--root"]).returncode, 2)

        no_main = self.base / "no-main.git"
        subprocess.run(["git", "init", "--bare", "-b", "feature", str(no_main)], env=self.git_env, check=True)
        no_main_client = self.base / "no-main-client"
        subprocess.run(["git", "clone", str(no_main), str(no_main_client)], env=self.git_env, check=True)
        self.assertEqual(self.run_helper(["--root", str(no_main_client)]).returncode, 2)

        bad_origin = self.base / "bad-origin-client"
        subprocess.run(["git", "clone", str(self.origin), str(bad_origin)], env=self.git_env, check=True)
        self.git(bad_origin, "remote", "set-url", "origin", "/nonexistent/repo.git")
        self.assertEqual(self.run_helper(["--root", str(bad_origin)]).returncode, 2)

        # Shim git to verify malformed length, extra columns, and multiple lines fail closed
        shim_dir = self.base / "shim_git"
        shim_dir.mkdir()
        shim_git = shim_dir / "git"
        shim_git.write_text("""#!/usr/bin/env bash
if [ "${3:-}" = "ls-remote" ]; then
  case "${SHIM_MODE:-}" in
    bad_hex) printf '%s\\trefs/heads/main\\n' "$(printf 'a%.0s' {1..41})" ;;
    extra_col) printf '%s\\trefs/heads/main\\textra_col\\n' "$(printf 'a%.0s' {1..40})" ;;
    multi_line) printf '%s\\trefs/heads/main\\n%s\\trefs/heads/main\\n' "$(printf 'a%.0s' {1..40})" "$(printf 'b%.0s' {1..40})" ;;
  esac
  exit 0
fi
exec /usr/bin/git "$@"
""", encoding="utf-8")
        shim_git.chmod(shim_git.stat().st_mode | stat.S_IXUSR)

        shim_env = self.git_env.copy()
        shim_env["PATH"] = f"{shim_dir}:{shim_env['PATH']}"
        for mode in ["bad_hex", "extra_col", "multi_line"]:
            shim_env["SHIM_MODE"] = mode
            res = self.run_helper(["--read-only"], env=shim_env)
            self.assertEqual(res.returncode, 2, f"Mode {mode} should fail closed")
            self.assertEqual(res.stdout.strip(), "")

class TestWorkflowPreflights(BaseGitTestCase):
    def setUp(self):
        super().setUp()
        self.bin_dir = self.base / "bin"
        self.bin_dir.mkdir()

        fake_gh = self.bin_dir / "gh"
        fake_gh.write_text("""#!/usr/bin/env bash
case "$1" in
  --version) echo "gh version 2.100.0 (test)" ;;
  auth) [ "${FAKE_AUTH_FAIL:-0}" = "1" ] && exit 1 || exit 0 ;;
  repo) echo '{"nameWithOwner":"acme/repo"}' ;;
  issue) echo '{"title":"document retry policy","labels":[]}' ;;
  pr) echo '[]' ;;
  webhook) exit 0 ;;
  *) exit 0 ;;
esac
""", encoding="utf-8")
        fake_gh.chmod(fake_gh.stat().st_mode | stat.S_IXUSR)

        fake_harness = self.bin_dir / "do-harness"
        fake_harness.write_text("""#!/usr/bin/env bash
case "$1" in
  pr) exit 0 ;;
  task) echo '{"title":"test task","status":"pending"}' ;;
  *) exit 0 ;;
esac
""", encoding="utf-8")
        fake_harness.chmod(fake_harness.stat().st_mode | stat.S_IXUSR)

        self.wf_env = self.git_env.copy()
        self.wf_env["PATH"] = f"{self.bin_dir}:{self.wf_env['PATH']}"
        self.wf_env["DO_HARNESS_BIN"] = str(fake_harness)

    def test_start_item_dry_run_and_advance(self):
        commit_b = self.advance_origin("commit B")

        # 1. Dry-run: observes advertised commit B without fetching or creating branch
        res = subprocess.run(
            ["bash", str(START_ITEM), "--issue", "62", "--dry-run"],
            cwd=str(self.client), env=self.wf_env, capture_output=True, text=True
        )
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn("dry-run:", res.stdout)
        self.assertIn("issue-62-document-retry-policy", res.stdout)
        self.assertIn(f"@{commit_b}", res.stdout)
        self.assertIn("(advertised only; not fetched)", res.stdout)

        # Branch absent, origin/main not fetched
        self.assertEqual(self.git(self.client, "branch", "--list", "issue-62*").stdout.strip(), "")
        self.assertEqual(self.git(self.client, "rev-parse", "refs/remotes/origin/main").stdout.strip(), self.commit_a)

        # 3. Start item: cuts new branch from commit B
        res = subprocess.run(["bash", str(START_ITEM), "--issue", "62"], cwd=str(self.client), env=self.wf_env, capture_output=True, text=True)
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn("started: issue #62 on branch issue-62-document-retry-policy", res.stdout)
        self.assertEqual(self.git(self.client, "rev-parse", "HEAD").stdout.strip(), commit_b)

        # 4. Add work on branch (commit C)
        (self.client / "fix.txt").write_text("item work\n", encoding="utf-8")
        self.git(self.client, "add", "fix.txt")
        self.git(self.client, "commit", "-m", "commit C")
        commit_c = self.git(self.client, "rev-parse", "HEAD").stdout.strip()

        # Switch to main and advance origin to commit D
        self.git(self.client, "checkout", "main")
        commit_d = self.advance_origin("commit D")

        # 3. Resume: dry-run observes D, normal resume refreshes origin/main to D
        res_dry = subprocess.run(["bash", str(START_ITEM), "--issue", "62", "--dry-run"], cwd=str(self.client), env=self.wf_env, capture_output=True, text=True)
        self.assertEqual(res_dry.returncode, 0, res_dry.stderr)
        self.assertIn("dry-run: resume branch issue-62-document-retry-policy", res_dry.stdout)
        self.assertIn(f"@{commit_d}", res_dry.stdout)
        self.assertIn("(advertised only; not fetched)", res_dry.stdout)

        res_resume = subprocess.run(["bash", str(START_ITEM), "--issue", "62"], cwd=str(self.client), env=self.wf_env, capture_output=True, text=True)
        self.assertEqual(res_resume.returncode, 0, res_resume.stderr)
        self.assertIn("resumed: issue #62 on branch issue-62-document-retry-policy", res_resume.stdout)
        self.assertEqual(self.git(self.client, "rev-parse", "HEAD").stdout.strip(), commit_c)
        self.assertEqual(self.git(self.client, "rev-parse", "refs/remotes/origin/main").stdout.strip(), commit_d)

    def test_preflight_scripts(self):
        commit_b = self.advance_origin("commit B")

        # 1. Preflight dry-run (issue-to-pr & pr-triage)
        for script in [ISSUE_PREFLIGHT, PR_PREFLIGHT]:
            res = subprocess.run(["bash", str(script), "--dry-run"], cwd=str(self.client), env=self.wf_env, capture_output=True, text=True)
            self.assertEqual(res.returncode, 0, f"{script.name}: {res.stderr}")

        # origin/main was not fetched during dry-run
        self.assertEqual(self.git(self.client, "rev-parse", "refs/remotes/origin/main").stdout.strip(), self.commit_a)

        # 2. Preflight normal run: fetches origin/main for BOTH scripts
        for script in [ISSUE_PREFLIGHT, PR_PREFLIGHT]:
            res = subprocess.run(["bash", str(script)], cwd=str(self.client), env=self.wf_env, capture_output=True, text=True)
            self.assertEqual(res.returncode, 0, f"{script.name}: {res.stderr}")

        self.assertEqual(self.git(self.client, "rev-parse", "refs/remotes/origin/main").stdout.strip(), commit_b)

        # 3. Dirty state: preflight preserves dirty state byte-for-byte on both scripts
        dirty_file = self.client / "dirty_preflight.txt"
        dirty_file.write_text("preflight dirty content\n", encoding="utf-8")
        readme = self.client / "README.md"
        readme.write_text("modified for preflight\n", encoding="utf-8")
        for script in [ISSUE_PREFLIGHT, PR_PREFLIGHT]:
            res = subprocess.run(["bash", str(script), "--dry-run"], cwd=str(self.client), env=self.wf_env, capture_output=True, text=True)
            self.assertEqual(res.returncode, 0, f"{script.name}: {res.stderr}")
            self.assertEqual(dirty_file.read_text(encoding="utf-8"), "preflight dirty content\n")
            self.assertEqual(readme.read_text(encoding="utf-8"), "modified for preflight\n")

        # 4. Preflight fails closed when origin is broken/missing
        no_remote = self.base / "no-remote-preflight"
        subprocess.run(["git", "init", str(no_remote)], env=self.git_env, check=True)
        for script in [ISSUE_PREFLIGHT, PR_PREFLIGHT]:
            res = subprocess.run(["bash", str(script)], cwd=str(no_remote), env=self.wf_env, capture_output=True, text=True)
            self.assertEqual(res.returncode, 2, f"{script.name} should fail closed")

        # 5. Preflight fails closed when latest-main.sh helper is missing
        fake_skill_dir = self.base / "fake_skill"
        (fake_skill_dir / "scripts").mkdir(parents=True)
        fake_script = fake_skill_dir / "scripts" / "preflight.sh"
        fake_script.write_text(ISSUE_PREFLIGHT.read_text(encoding="utf-8"), encoding="utf-8")
        fake_script.chmod(fake_script.stat().st_mode | stat.S_IXUSR)
        res = subprocess.run(["bash", str(fake_script)], cwd=str(self.client), env=self.wf_env, capture_output=True, text=True)
        self.assertEqual(res.returncode, 2, "Missing helper must fail closed with exit 2")
        self.assertIn("latest-main helper not found", res.stderr)


if __name__ == "__main__":
    unittest.main()
