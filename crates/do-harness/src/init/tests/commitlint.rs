//! The managed `commit-msg` hook runs `scripts/check-commitlint.sh` in every
//! pack, so `init` must scaffold it everywhere, not only in the rust pack.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn init_scaffolds_the_commitlint_script_in_every_pack() {
    for lang in [
        Language::Rust,
        Language::Generic,
        Language::Web,
        Language::Node,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let report = init_workspace(dir.path(), &opts(Some(lang))).await.unwrap();

        assert!(
            report
                .written
                .contains(&"scripts/check-commitlint.sh".to_owned()),
            "the commit-msg hook dependency must be written for language {lang:?}"
        );
        let script = dir.path().join("scripts/check-commitlint.sh");
        assert!(
            script.is_file(),
            "the script must exist for language {lang:?}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&script).unwrap().permissions().mode();
            assert!(
                mode & 0o111 != 0,
                "the script must be executable for language {lang:?}"
            );
        }
    }
}
