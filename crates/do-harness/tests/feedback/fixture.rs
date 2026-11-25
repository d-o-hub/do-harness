//! Fake-`gh` fixture and harness command for the feedback tests.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
pub(super) const EXEC_PERMS: u32 = 0o755;
pub(super) const MIN_INDEPENDENT_FIRES: usize = 3;
pub(super) const SAMPLE_TIMESTAMP: i64 = 1_700_000_000;
pub(super) const MOCK_ISSUE_NUMBER: u64 = 99;

pub(super) struct Fixture {
    pub(super) _temp: tempfile::TempDir,
    pub(super) root: PathBuf,
    pub(super) bin_dir: PathBuf,
}

pub(super) fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir_all(&root).unwrap();

    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let fake_gh = bin_dir.join("gh");
    let script = format!(
        r#"#!/bin/sh
ROOT="{}"
case "$1" in
    issue)
        case "$2" in
            list)
                if [ -f "$ROOT/fail_gh" ]; then
                    echo "gh: authentication failed or rate limited" >&2
                    exit 1
                fi
                if [ -f "$ROOT/mock_issues.json" ]; then
                    cat "$ROOT/mock_issues.json"
                else
                    echo "[]"
                fi
                exit 0
                ;;
            create)
                if [ -f "$ROOT/fail_gh" ]; then
                    echo "gh: authentication failed or rate limited" >&2
                    exit 1
                fi
                cat - > "$ROOT/last_created_body.txt"
                echo "$@" > "$ROOT/last_created_argv.txt"
                echo "https://github.com/d-o-hub/do-harness/issues/42"
                exit 0
                ;;
        esac
        ;;
esac
echo "unknown fake gh invocation: $@" >&2
exit 1
"#,
        root.display()
    );
    fs::write(&fake_gh, script).unwrap();
    let mut perms = fs::metadata(&fake_gh).unwrap().permissions();
    perms.set_mode(EXEC_PERMS);
    fs::set_permissions(&fake_gh, perms).unwrap();

    fs::write(
        root.join("do-harness.toml"),
        "language = \"rust\"\n[[sensors]]\nname = \"clippy\"\nargv = [\"cargo\", \"clippy\"]\n",
    )
    .unwrap();

    Fixture {
        _temp: temp,
        root,
        bin_dir,
    }
}

pub(super) fn harness(fx: &Fixture) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(&fx.root);
    let old_path = std::env::var("PATH").unwrap_or_default();
    cmd.env("PATH", format!("{}:{}", fx.bin_dir.display(), old_path));
    cmd
}
