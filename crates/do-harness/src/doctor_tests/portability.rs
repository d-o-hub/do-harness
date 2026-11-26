//! Helper-portability reports: which shipped helpers are native and whether
//! the effective configuration still requires a POSIX shell for one.
//!
//! `loc` is asserted separately because its native advice is the *strict*
//! command: the report-only query exits 0 on an over-limit tree, which is how a
//! 535-line file shipped through every gate.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::doctor::checks::helper_portability;

use super::fake_repo_with_git;

/// The default pack (no config file) reports every native helper as native
/// with the native argv, never as shell-routed.
#[tokio::test(flavor = "current_thread")]
async fn helper_portability_reports_native_defaults() {
    let (_temp, root) = fake_repo_with_git();
    let rows = helper_portability(&root).await;
    for row in &rows {
        assert!(row.native, "{} must be native", row.name);
        assert!(!row.shell, "{} must not route through the shell", row.name);
        assert_eq!(row.argv[0], "do-harness");
    }
    let loc = rows.iter().find(|row| row.name == "loc").unwrap();
    assert_eq!(
        loc.native_argv,
        vec!["do-harness", "loc", "--strict"],
        "the loc advice must name the gate, not the report-only query"
    );
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    for name in ["commitlint", "deps", "audit", "loc"] {
        assert!(names.contains(&name), "missing helper {name}");
    }
}

/// A config that still routes a helper through `bash` is reported as shell-
/// required, with the native alternative available.
#[tokio::test(flavor = "current_thread")]
async fn helper_portability_flags_shell_configured_sensors() {
    let (_temp, root) = fake_repo_with_git();
    std::fs::write(
        root.join("do-harness.toml"),
        "[[sensors]]\nname = \"commitlint\"\nargv = [\"bash\", \"scripts/check-commitlint.sh\"]\n",
    )
    .unwrap();
    let rows = helper_portability(&root).await;
    let commitlint = rows.iter().find(|row| row.name == "commitlint").unwrap();
    assert!(commitlint.native);
    assert!(commitlint.shell);
    assert_eq!(commitlint.argv, vec!["bash", "scripts/check-commitlint.sh"]);
    assert_eq!(
        commitlint.native_argv,
        vec!["do-harness", "helpers", "commitlint"],
        "the warning must name the native command to switch to"
    );
    let loc = rows.iter().find(|row| row.name == "loc").unwrap();
    assert_eq!(loc.native_argv, vec!["do-harness", "loc", "--strict"]);
}
